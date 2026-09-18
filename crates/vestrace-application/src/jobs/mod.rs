//! The job queue's port.
//!
//! # What is not here
//!
//! A `Worker` used to sit beside this module. Its whole body was
//! `lease_next()` … `// Process job execution` … `complete(job.id)`: it marked a
//! job completed having executed nothing, and it was wired into the worker
//! binary's poll loop. Nothing ever enqueued a job, so the lie stayed latent —
//! but any future producer would have watched its work vanish into a row that
//! said `completed`.
//!
//! Asynchronous work has two real mechanisms: `run_work_items`, which the run
//! worker leases and executes, and the outbox, which now has a dispatcher. This
//! port is retained because the `jobs` table and the `jobs.no_dead_letters`
//! invariant still read it, and removing a table is a separate decision from
//! removing a stub that claimed to drain it.

mod ports;

pub use ports::*;
