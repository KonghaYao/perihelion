pub mod config;
pub mod full;
pub mod invariant;
pub mod micro;
pub mod re_inject;

pub use config::CompactConfig;
pub use full::{full_compact, FullCompactResult};
pub use invariant::{
    adjust_index_to_preserve_invariants,
    group_messages_by_round,
    MessageRound,
};
pub use micro::micro_compact_enhanced;
pub use re_inject::{re_inject, ReInjectResult};
