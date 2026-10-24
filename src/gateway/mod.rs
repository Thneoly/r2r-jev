//! The gateway shape: an MCP proxy that sits between an agent and real
//! upstream MCP tool servers. Every `tools/call` passes through here before
//! it reaches an upstream — the enforcement leg of
//! `Judgment -> Evidence -> Admission -> R2R -> Enforcement`.
//!
//! P0 scope: spawn upstreams, merge tool catalogs, proxy calls.
//! Governance gating (observe + decide before forwarding, evidence-bearing
//! denies) lands on top of this skeleton.

pub mod config;
pub mod job;
pub mod service;
