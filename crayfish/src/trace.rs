use once_cell::sync::Lazy;
use rustc_hash::FxHashMap;
use rustc_hash::FxHasher;
use std::cell::UnsafeCell;
use std::hash::Hash;
use std::hash::Hasher;
use std::sync::Mutex;
use std::thread;
use std::time;

pub fn timer_now() -> time::Instant {
    time::Instant::now()
}

#[derive(Copy, Clone)]
pub struct Span {
    span_name: &'static str,
    module: &'static str,
    line: u32,
    hash: SpanId,
}

impl Span {
    pub fn new(span_name: &'static str, module: &'static str, line: u32) -> Span {
        let mut hs = FxHasher::default();
        span_name.hash(&mut hs);
        module.hash(&mut hs);
        line.hash(&mut hs);
        let hash = hs.finish();
        Span {
            span_name,
            module,
            line,
            hash,
        }
    }
}

pub fn register_span(sp: &Span) {
    let mut st = GLOBAL_SPAN_TABLE.lock().unwrap();
    // might register multiple time for the same localtion (because of generic)
    st.insert(sp.hash, sp.clone());
}

pub fn profile_span_submit(sp: &Span, dur: time::Duration) {
    let nano_sec = dur.as_nanos() as u64;
    let id = sp.hash;

    visit(|table: &mut DurationTable| {
        table.entry(id).or_default().insert(nano_sec);
    });
}

struct DurationRecord {
    min: u64, // in nano secs
    max: u64,
    acc: u64,
    count: u64,
}

impl Default for DurationRecord {
    fn default() -> Self {
        DurationRecord {
            min: u64::MAX,
            max: 0,
            acc: 0,
            count: 0,
        }
    }
}

impl DurationRecord {
    fn insert(&mut self, data: u64) {
        if self.min > data {
            self.min = data
        }
        if self.max < data {
            self.max = data
        }
        self.acc += data;
        self.count += 1;
    }

    fn update(&mut self, other: &Self) {
        self.min = u64::min(self.min, other.min);
        self.max = u64::max(self.max, other.max);
        self.acc += other.acc;
        self.count += other.count;
    }
}

type SpanId = u64;
type DurationTable = FxHashMap<SpanId, DurationRecord>;

static GLOBAL_SPAN_TABLE: Lazy<Mutex<FxHashMap<SpanId, Span>>> =
    Lazy::new(|| Mutex::new(FxHashMap::default()));
static GLOBAL_METRIC_TABLE: Lazy<Mutex<DurationTable>> =
    Lazy::new(|| Mutex::new(DurationTable::default()));

struct ThreadLocalMetricTable {
    inner: DurationTable,
    thread_id: thread::ThreadId,
}

impl Default for ThreadLocalMetricTable {
    fn default() -> Self {
        ThreadLocalMetricTable {
            inner: DurationTable::default(),
            thread_id: thread::current().id(),
        }
    }
}

impl Drop for ThreadLocalMetricTable {
    fn drop(&mut self) {
        print_metric_table(&format!("{:?}", self.thread_id), &self.inner);
        // merge to the global
        let mut t = GLOBAL_METRIC_TABLE.lock().expect("global metric table lock poisioned");
        for (k, v) in self.inner.iter() {
            t.entry(k.clone()).or_default().update(v);
        }
    }
}

fn print_metric_table(table_name: &str, table: &DurationTable) {
    let show_duration = |nano_sec: u64| -> String {
        if nano_sec < 1000 {
            format!("{}ns", nano_sec)
        } else if nano_sec < 1_000_000 {
            format!("{:.3}us", nano_sec as f64 / 1000.0)
        } else if nano_sec < 1_000_000_000 {
            format!("{:.6}ms", nano_sec as f64 / 1e6)
        } else {
            format!("{:.6}s", nano_sec as f64 / 1e9)
        }
    };
    let mut orderd_record: Vec<_> = table.iter().collect();
    orderd_record.sort_unstable_by_key(|(_, record)| std::cmp::Reverse(record.acc));
    let here = crate::global_id::here();

    let mut show: String = String::default();
    let place_and_thread = format!("[{}:{}]", here, table_name);
    let span_table = GLOBAL_SPAN_TABLE.lock().expect("global span table lock poisoned").clone();
    for (id, r) in orderd_record {
        let span = span_table[id];
        show.push_str(&format!(
            "\n{} {}@{}:{} => total:{} min: {} max: {} avg: {} count {}",
            place_and_thread,
            span.span_name,
            span.module,
            span.line,
            show_duration(r.acc),
            show_duration(r.min),
            show_duration(r.max),
            show_duration(r.acc / r.count),
            r.count
        ));
    }
    use std::io::Write;
    let stderr = std::io::stderr();
    let mut stderr_h = stderr.lock();
    // should not panic since this function is in drop
    stderr_h.write_all(show.as_bytes()).unwrap_or(());
    stderr_h.flush().unwrap_or(());
}

pub fn print_profiling() {
    let g = GLOBAL_METRIC_TABLE.lock().unwrap();
    print_metric_table("Summary", &*g);
}

thread_local! {
    static METRIC_TABLE: UnsafeCell<Option<ThreadLocalMetricTable>> = UnsafeCell::new(None);
}

fn visit<F>(mut f: F)
where
    F: for<'a> FnMut(&'a mut DurationTable),
{
    METRIC_TABLE.with(|cell| {
        let maybe_table = unsafe { &mut *cell.get() };
        let table = maybe_table.get_or_insert_with(|| ThreadLocalMetricTable::default());
        f(&mut table.inner);
    })
}
