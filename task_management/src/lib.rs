//! 基于vsched的任务管理库，提供线程和协程的创建、调度和管理功能。
//!
//! 任务调度功能的使用者应直接依赖本库。

#![no_std]
#![deny(missing_docs)]

extern crate alloc;

pub mod interface;
pub mod sched;
pub mod task;
pub mod task_api;
pub mod task_inner_ext;
pub mod wait_queue;
