# n64-rs

Modular Nintendo 64 emulator written in Rust.

Same playbook as `x360-rs`: clean-room, genuine-only faults, fault-chase loop,
ISA verified against authority, small verified commits.

## Layout

- `crates/common` — errors, config, logging
- `crates/memory` — RDRAM + mirrors, SP DMEM/IMEM, MMIO guard
- `crates/cpu` — VR4300 (MIPS III) interpreter + RSP stub, JIT-ready traits
- `crates/loader` — .z64/.n64/.v64 normalize + header parse
- `crates/gpu` — RDP command decode, VI state, software framebuffer
- `crates/audio` — AI tracking, null/host backends
- `crates/filesystem` — EEPROM/SRAM/FlashRAM/Controller Pak saves
- `crates/kernel` — PIF boot, MI interrupts, libultra HLE stubs
- `crates/network` — stub (N64 has no net; loopback harness only)
- `crates/debugger` — disassembly, breakpoints, tracing
- `crates/frontend` — CLI now, desktop UI at M2+
- `crates/emutests` — integration and regression tests

## Status

Milestone-based. See `docs/DESIGN.md` and `docs/COMPATIBILITY.md`.
No subsystem is marked complete without tests and measured behavior.
Unsupported paths return explicit errors and are logged.

## Build

```
cargo build
cargo test
cargo run -p n64-frontend -- --help
```

## ROMs

Firmware/ROMs never enter git. Retail tests gate on `$N64_ROM_FILE`.
