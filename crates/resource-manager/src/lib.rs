//! Resource manager monitoring memory, CPU budgets, and system pressure.

pub mod budget;
pub mod cpu_monitor;
pub mod memory_monitor;
pub mod pressure;
pub mod scheduler;
pub mod watchdog;

pub use budget::MemoryBudget;
pub use cpu_monitor::CpuMonitor;
pub use memory_monitor::{MemoryMonitor, ProcessMemoryBytes};
pub use pressure::evaluate_memory_pressure;
pub use scheduler::{ResourceScheduler, ResourceTickReport};
pub use watchdog::Watchdog;
