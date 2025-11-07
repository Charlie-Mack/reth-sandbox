// src/metrics.rs
use once_cell::sync::Lazy;
use std::{
    borrow::Cow,
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Default, Clone, Copy)]
struct Accum {
    inclusive: Duration, // full span duration
    exclusive: Duration, // span minus time spent in child sections
    count: u64,
}

// We allow both 'static and owned names via Cow for flexibility.
#[derive(Clone, Eq, PartialEq, Hash, PartialOrd, Ord)]
struct Key(Cow<'static, str>);

impl From<&'static str> for Key {
    fn from(value: &'static str) -> Self {
        Self(Cow::Borrowed(value))
    }
}

impl From<String> for Key {
    fn from(value: String) -> Self {
        Self(Cow::Owned(value))
    }
}

impl From<Cow<'static, str>> for Key {
    fn from(value: Cow<'static, str>) -> Self {
        Self(value)
    }
}

impl Key {
    fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

static SECTIONS: Lazy<Mutex<HashMap<Key, Accum>>> = Lazy::new(|| Mutex::new(HashMap::new()));
static BLOCK_SECTIONS: Lazy<Mutex<HashMap<Key, HashMap<Key, Accum>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

// -------- Total run timer --------
static RUN: Lazy<Mutex<Option<Instant>>> = Lazy::new(|| Mutex::new(None));
static RUN_TOTAL: Lazy<Mutex<Duration>> = Lazy::new(|| Mutex::new(Duration::ZERO));

pub fn run_start() {
    *RUN.lock().unwrap() = Some(Instant::now());
}

pub fn run_end() {
    if let Some(start) = RUN.lock().unwrap().take() {
        *RUN_TOTAL.lock().unwrap() = start.elapsed();
    }
}

pub fn run_total() -> Duration {
    *RUN_TOTAL.lock().unwrap()
}

// -------- Exclusive span tracking (nesting-aware) --------
// We maintain a thread-local stack of active spans. When a child starts,
// we "pause" the parent (account to parent.exclusive), and resume it when
// the child ends. That yields *exclusive* time without double counting.
thread_local! {
    static STACK: std::cell::RefCell<Vec<ActiveSpan>> = Default::default();
}

struct ActiveSpan {
    key: Key,
    block: Option<Key>,
    start: Instant,       // wall-clock start of this span (for inclusive)
    last_resume: Instant, // when we last resumed exclusive accumulation
    paused_exclusive: Duration,
}

impl ActiveSpan {
    fn new(key: Key, block: Option<Key>) -> Self {
        let now = Instant::now();
        Self {
            key,
            block,
            start: now,
            last_resume: now,
            paused_exclusive: Duration::ZERO,
        }
    }
}

pub struct SectionTimer {
    key: Key,
    block: Option<Key>,
    // field exists just so the type is not ZST; logic is in Drop + thread-local stack
    _private: (),
}

/// Start a nested (exclusive) timing section.
/// Use: `let _t = time_section!("execute_transaction");`
impl SectionTimer {
    #[inline]
    pub fn new_static(name: &'static str) -> Self {
        start_section(name.into(), None)
    }
    #[inline]
    pub fn new_owned(name: String) -> Self {
        start_section(name.into(), None)
    }
    #[inline]
    pub fn new_grouped(
        block_label: impl Into<Key>,
        name: impl Into<Key>,
    ) -> Self {
        start_section(name.into(), Some(block_label.into()))
    }
}

fn start_section(key: Key, block: Option<Key>) -> SectionTimer {
    STACK.with(|stack| {
        let mut st = stack.borrow_mut();
        let now = Instant::now();
        // Pause the current top (for exclusive accounting)
        if let Some(parent) = st.last_mut() {
            // Add parent's exclusive time up to now
            parent.paused_exclusive += now - parent.last_resume;
        }
        // Push this section
        st.push(ActiveSpan::new(key.clone(), block.clone()));
    });
    SectionTimer { key, block, _private: () }
}

impl Drop for SectionTimer {
    fn drop(&mut self) {
        STACK.with(|stack| {
            let mut st = stack.borrow_mut();
            let now = Instant::now();
            let mut span = st.pop().expect("unbalanced SectionTimer");

            // Finalize this span's inclusive and exclusive times
            let inclusive = now - span.start;
            let exclusive = span.paused_exclusive + (now - span.last_resume);

            // Record into global table
            {
                let mut map = SECTIONS.lock().unwrap();
                let entry = map.entry(self.key.clone()).or_insert_with(Accum::default);
                entry.inclusive += inclusive;
                entry.exclusive += exclusive;
                entry.count += 1;
            }

            if let Some(block_key) = self.block.clone() {
                let mut map = BLOCK_SECTIONS.lock().unwrap();
                let block_entry = map.entry(block_key).or_insert_with(HashMap::new);
                let entry = block_entry.entry(self.key.clone()).or_insert_with(Accum::default);
                entry.inclusive += inclusive;
                entry.exclusive += exclusive;
                entry.count += 1;
            }

            // Resume parent (set its last_resume to now)
            if let Some(parent) = st.last_mut() {
                parent.last_resume = now;
            }
        });
    }
}

// ---------- Convenience macros ----------
#[macro_export]
macro_rules! time_section {
    // 1) Static string (no allocation)
    ($name:literal) => {
        $crate::metrics::SectionTimer::new_static($name)
        };
    // 2) format!-style usage (allocates a String)
    ($fmt:literal, $($arg:tt)+) => {{
        $crate::metrics::SectionTimer::new_owned(format!($fmt, $($arg)+))
    }};
    // 3) Any &str/String expr
    ($name:expr) => {{
        $crate::metrics::SectionTimer::new_owned($name.to_string())
    }};
}
#[macro_export]
macro_rules! time_section_owned {
    ($name_expr:expr) => {
        $crate::metrics::SectionTimer::new_owned($name_expr.to_string())
    };
}

#[macro_export]
macro_rules! time_block_section {
    ($block:expr, $name:literal) => {{
        $crate::metrics::SectionTimer::new_grouped(format!("block {}", $block), $name)
    }};
    ($block:expr, $fmt:literal, $($arg:tt)+) => {{
        $crate::metrics::SectionTimer::new_grouped(
            format!("block {}", $block),
            format!($fmt, $($arg)+)
        )
    }};
    ($block:expr, $name:expr) => {{
        $crate::metrics::SectionTimer::new_grouped(
            format!("block {}", $block),
            $name
        )
    }};
}

// ---------- Async helper ----------
pub async fn time_async_section<F, T>(name: &'static str, fut: F) -> T
where
    F: std::future::Future<Output = T>,
{
    let _t = SectionTimer::new_static(name);
    fut.await
}

// ---------- Printing ----------
pub fn print_section_summary() {
    // Prefer to call run_end() before printing.
    let total = run_total();

    let map = SECTIONS.lock().unwrap();

    // width calc
    let mut name_w = "Section".len();
    for k in map.keys() {
        name_w = name_w.max(k.0.len());
    }

    println!();
    println!("{:-<1$}", "", name_w + 56);
    println!(
        "{:<name_w$}  {:>10}  {:>14}  {:>14}  {:>14}",
        "Section",
        "Count",
        "Incl (ms)",
        "Excl (ms)",
        "Avg Excl (ms)",
        name_w = name_w
    );
    println!("{:-<1$}", "", name_w + 56);

    let mut sum_exclusive = 0.0f64;
    for (key, acc) in map.iter() {
        let incl_ms = acc.inclusive.as_secs_f64() * 1000.0;
        let excl_ms = acc.exclusive.as_secs_f64() * 1000.0;
        sum_exclusive += excl_ms;
        let avg_excl = if acc.count > 0 {
            excl_ms / acc.count as f64
        } else {
            0.0
        };
        println!(
            "{:<name_w$}  {:>10}  {:>14.3}  {:>14.3}  {:>14.3}",
            &key.0,
            acc.count,
            incl_ms,
            excl_ms,
            avg_excl,
            name_w = name_w
        );
    }
    println!("{:-<1$}", "", name_w + 56);

    if total > Duration::ZERO {
        let total_ms = total.as_secs_f64() * 1000.0;
        let unattributed_ms = (total_ms - sum_exclusive).max(0.0);
        println!("TOTAL (wall)      {:>10}  {:>14.3}", "", total_ms);
        println!("UNATTRIBUTED (ms) {:>10}  {:>14.3}", "", unattributed_ms);
    }

    drop(map);

    let block_map = BLOCK_SECTIONS.lock().unwrap();
    if block_map.is_empty() {
        return;
    }

    println!("\nPer-block breakdown:");

    let mut blocks: Vec<_> = block_map.iter().collect();
    blocks.sort_by(|(a, _), (b, _)| a.as_str().cmp(b.as_str()));

    for (block_key, sections) in blocks {
        println!();
        println!("Block {}:", block_key.as_str());

        let mut name_w = "Section".len();
        for name in sections.keys() {
            name_w = name_w.max(name.as_str().len());
        }

        println!("{:-<1$}", "", name_w + 56);
        println!(
            "{:<name_w$}  {:>10}  {:>14}  {:>14}  {:>14}",
            "Section",
            "Count",
            "Incl (ms)",
            "Excl (ms)",
            "Avg Excl (ms)",
            name_w = name_w
        );
        println!("{:-<1$}", "", name_w + 56);

        let mut section_rows: Vec<_> = sections.iter().collect();
        section_rows.sort_by(|(_, a), (_, b)| b.exclusive.cmp(&a.exclusive));

        for (name, acc) in section_rows {
            let incl_ms = acc.inclusive.as_secs_f64() * 1000.0;
            let excl_ms = acc.exclusive.as_secs_f64() * 1000.0;
            let avg_excl = if acc.count > 0 {
                excl_ms / acc.count as f64
            } else {
                0.0
            };
            println!(
                "{:<name_w$}  {:>10}  {:>14.3}  {:>14.3}  {:>14.3}",
                name.as_str(),
                acc.count,
                incl_ms,
                excl_ms,
                avg_excl,
                name_w = name_w
            );
        }
        println!("{:-<1$}", "", name_w + 56);
    }
}
