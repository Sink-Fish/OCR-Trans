//! What this processor is like, for choosing thread counts and for the log: performance vs. efficiency cores and
//! the SIMD instruction sets the CPU reports. Read from the OS on every machine — no model is assumed.
//!
//! - Windows: `GetLogicalProcessorInformationEx(RelationProcessorCore)`; each core carries an `EfficiencyClass`,
//!   higher = faster. On a hybrid CPU the performance cores are the ones with the highest class.
//! - macOS: `hw.perflevel0.physicalcpu` (performance cores) and `hw.physicalcpu`.
//! - elsewhere: half the logical processors.

use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CpuInfo {
    /// physical cores of the fastest class (all physical cores on a non-hybrid CPU)
    pub performance_cores: usize,
    pub physical_cores: usize,
    /// cores of more than one class (performance + efficiency)
    pub hybrid: bool,
    /// SIMD features the CPU reports, e.g. "avx2 fma f16c"
    pub features: String,
}

pub fn info() -> &'static CpuInfo {
    static INFO: OnceLock<CpuInfo> = OnceLock::new();
    INFO.get_or_init(|| {
        let (performance_cores, physical_cores, hybrid) = cores();
        CpuInfo { performance_cores, physical_cores, hybrid, features: features() }
    })
}

/// Threads for llama.cpp: (token generation, prompt processing). `OCRTRANS_LLM_THREADS=gen[,batch]` overrides.
pub fn llm_threads() -> (usize, usize) {
    if let Some(t) = std::env::var("OCRTRANS_LLM_THREADS").ok().and_then(|v| parse_threads(&v)) {
        return t;
    }
    let i = info();
    threads_for(i.performance_cores, i.physical_cores, i.hybrid)
}

fn parse_threads(v: &str) -> Option<(usize, usize)> {
    let mut it = v.split(',').map(|s| s.trim().parse::<usize>().ok().filter(|&n| n > 0));
    let gen = it.next()??;
    let batch = it.next().flatten().unwrap_or(gen);
    Some((gen, batch))
}

/// Token generation is bound by memory bandwidth and by the slowest thread (work is split evenly), so it runs on
/// the performance cores only; efficiency cores would hold every step up. Without a hybrid split: the physical
/// cores minus one or two for the rest of the system, at least 2. Prompt processing is compute-bound: on a
/// non-hybrid CPU it may use every physical core. At most 8 either way (more stops helping a 1.7B model).
pub fn threads_for(performance: usize, physical: usize, hybrid: bool) -> (usize, usize) {
    let physical = physical.max(1);
    if hybrid {
        let gen = performance.clamp(2, 8);
        return (gen, gen);
    }
    let spare = if physical <= 4 { 1 } else { 2 };
    let gen = physical.saturating_sub(spare).clamp(2, 8);
    (gen, physical.clamp(2, 8))
}

/// (performance cores, physical cores, hybrid) from the efficiency class of every physical core.
pub fn classify(classes: &[u8]) -> (usize, usize, bool) {
    let Some(&top) = classes.iter().max() else { return (0, 0, false) };
    let perf = classes.iter().filter(|&&c| c == top).count();
    (perf, classes.len(), classes.iter().any(|&c| c != top))
}

#[cfg(target_os = "windows")]
fn cores() -> (usize, usize, bool) {
    use std::ffi::c_void;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLogicalProcessorInformationEx(relationship: i32, buffer: *mut c_void, length: *mut u32) -> i32;
    }
    const RELATION_PROCESSOR_CORE: i32 = 0;
    let mut len = 0u32;
    unsafe { GetLogicalProcessorInformationEx(RELATION_PROCESSOR_CORE, std::ptr::null_mut(), &mut len) };
    let mut buf = vec![0u8; len as usize];
    if len == 0 || unsafe { GetLogicalProcessorInformationEx(RELATION_PROCESSOR_CORE, buf.as_mut_ptr().cast(), &mut len) } == 0 {
        return fallback_cores();
    }
    // records: Relationship u32, Size u32, then PROCESSOR_RELATIONSHIP { Flags u8, EfficiencyClass u8, … }
    let mut classes = vec![];
    let mut off = 0usize;
    while off + 10 <= len as usize {
        let size = u32::from_le_bytes(buf[off + 4..off + 8].try_into().unwrap()) as usize;
        if size == 0 {
            break;
        }
        classes.push(buf[off + 9]);
        off += size;
    }
    match classify(&classes) {
        (0, _, _) => fallback_cores(),
        c => c,
    }
}

#[cfg(target_os = "macos")]
fn cores() -> (usize, usize, bool) {
    fn sysctl(name: &std::ffi::CStr) -> Option<usize> {
        extern "C" {
            fn sysctlbyname(name: *const std::ffi::c_char, old: *mut std::ffi::c_void, oldlen: *mut usize, new: *mut std::ffi::c_void, newlen: usize) -> i32;
        }
        let mut v: i32 = 0;
        let mut len = std::mem::size_of::<i32>();
        let r = unsafe { sysctlbyname(name.as_ptr(), (&mut v as *mut i32).cast(), &mut len, std::ptr::null_mut(), 0) };
        (r == 0 && v > 0).then_some(v as usize)
    }
    let physical = sysctl(c"hw.physicalcpu");
    match (sysctl(c"hw.perflevel0.physicalcpu"), physical) {
        (Some(p), Some(all)) => (p, all, p < all),
        (None, Some(all)) => (all, all, false),
        _ => fallback_cores(),
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn cores() -> (usize, usize, bool) {
    fallback_cores()
}

fn fallback_cores() -> (usize, usize, bool) {
    let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).div_ceil(2).max(1);
    (n, n, false)
}

fn features() -> String {
    let mut f: Vec<&str> = vec![];
    #[cfg(target_arch = "x86_64")]
    {
        macro_rules! has {
            ($($name:tt),*) => { $( if std::arch::is_x86_feature_detected!($name) { f.push($name); } )* };
        }
        has!("sse4.2", "avx", "avx2", "fma", "f16c", "bmi2", "avx512f", "avx512vnni");
    }
    #[cfg(target_arch = "aarch64")]
    {
        macro_rules! has {
            ($($name:tt),*) => { $( if std::arch::is_aarch64_feature_detected!($name) { f.push($name); } )* };
        }
        has!("neon", "dotprod", "fp16", "i8mm");
    }
    f.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hybrid_cpus_use_their_performance_cores() {
        // 4 performance + 8 efficiency + 2 low-power efficiency cores (classes 1 / 0 / 0)
        let classes = [vec![1u8; 4], vec![0u8; 10]].concat();
        assert_eq!(classify(&classes), (4, 14, true));
        assert_eq!(threads_for(4, 14, true), (4, 4));
        // 6 + 8 with three classes: only the top class counts
        let classes = [vec![2u8; 6], vec![1u8; 4], vec![0u8; 4]].concat();
        assert_eq!(classify(&classes), (6, 14, true));
        // a hybrid CPU with a single performance core still gets 2 threads
        assert_eq!(threads_for(1, 9, true), (2, 2));
    }

    #[test]
    fn uniform_cpus_leave_a_core_or_two_free() {
        assert_eq!(classify(&[0; 8]), (8, 8, false));
        assert_eq!(threads_for(8, 8, false), (6, 8));
        assert_eq!(threads_for(4, 4, false), (3, 4));
        assert_eq!(threads_for(2, 2, false), (2, 2));
        assert_eq!(threads_for(1, 1, false), (2, 2));
        assert_eq!(threads_for(32, 32, false), (8, 8));
        assert_eq!(classify(&[]), (0, 0, false));
    }

    #[test]
    fn thread_override() {
        assert_eq!(parse_threads("6"), Some((6, 6)));
        assert_eq!(parse_threads("4, 8"), Some((4, 8)));
        assert_eq!(parse_threads("0"), None);
        assert_eq!(parse_threads("x"), None);
    }

    #[test]
    fn this_machine_is_read() {
        let i = info();
        assert!(i.physical_cores >= 1 && i.performance_cores >= 1 && i.performance_cores <= i.physical_cores, "{i:?}");
        let (g, b) = llm_threads();
        assert!((2..=8).contains(&g) && (2..=8).contains(&b));
    }
}
