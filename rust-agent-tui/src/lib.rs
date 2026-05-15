//! TUI interface for Rust Agent - interactive terminal playground

#![allow(
    clippy::if_same_then_else,
    clippy::needless_range_loop,
    clippy::reversed_empty_ranges,
    clippy::let_underscore_future,
    clippy::question_mark,
    clippy::collapsible_else_if,
    clippy::ptr_arg,
    clippy::infallible_try_from
)]

pub mod acp;
pub mod app;
pub mod command;
pub mod config;
pub mod event;
pub mod i18n;
pub mod langfuse;
pub mod prompt;
pub mod thread;
pub mod ui;
