mod dispatch_queue_timing;
mod pe_core_arithmetic;
mod preemption_boundary;
mod sleep_jitter;
mod thread_lifecycle;
mod timer_coalescing;

pub use dispatch_queue_timing::DispatchQueueTimingSource;
pub use pe_core_arithmetic::PECoreArithmeticSource;
pub use preemption_boundary::PreemptionBoundarySource;
pub use sleep_jitter::SleepJitterSource;
pub use thread_lifecycle::ThreadLifecycleSource;
pub use timer_coalescing::TimerCoalescingSource;

use crate::source::EntropySource;

pub fn sources() -> Vec<Box<dyn EntropySource>> {
    vec![
        Box::new(DispatchQueueTimingSource),
        Box::new(PECoreArithmeticSource),
        Box::new(PreemptionBoundarySource),
        Box::new(SleepJitterSource),
        Box::new(ThreadLifecycleSource),
        Box::new(TimerCoalescingSource),
    ]
}
