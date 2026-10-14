# n64-rs Technical Design (Milestone 1)

## 1. References

- NEC VR4300 User's Manual: MIPS III, 64-bit GPRs, COP0, FPU, LL/SC,
  cache ops, exceptions (TLB, syscall, break, overflow).
- MIPS III ISA quick-reference: primary opcode + SPECIAL function +
  REGIMM tables. Decoder constants trace to these tables.
- SGI N64 specifics: RDRAM 4/8 MB, SP DMEM/IMEM 4 KiB each, MI/VI/AI/
  PI/SI/DP MMIO, PIF boot (first 4 KiB), RSP vector unit, RDP rasterizer.
- Prior work studied (read-only, never compiled in): ares/simple64/mupen
  behavior notes for boot sequence and save sizes. This project writes
  100% of its own code; format facts are cited per-field.

## 2. Dependency assessment

| Area | Candidate | License | Verdict |
|---|---|---|---|
| Logging | log + env_logger | MIT/Apache-2.0 | Selected. |
| Errors | thiserror | MIT/Apache-2.0 | Selected. |
| Config | serde + serde_json | MIT/Apache-2.0 | Selected. |
| Host GPU | wgpu | MIT/Apache-2.0 | Deferred to M5 (software fb until then). |
| GUI | eframe/egui | MIT/Apache-2.0 | Deferred to M2+ (CLI now). |
| Audio out | cpal | Apache-2.0 | Deferred, null backend default. |

Licensing: workspace BSD-3-Clause. All deps MIT/Apache-2.0 compatible.

## 3. Module boundaries

```
common <- memory <- cpu <- kernel <- frontend
common <- memory <- loader <- kernel
common <- memory <- gpu <- frontend
common <- audio <- kernel
common <- filesystem <- kernel <- frontend
common <- network <- frontend
common <- cpu/memory/kernel/gpu <- debugger <- frontend
```

One-directional. No cycles. `unsafe` only where documented (none in M1).

## 4. CPU plan

- Fetch/decode/execute with real delay slots + load-delay.
- Decoder returns `Decoded`; unknown -> `Unsupported` with exact fields.
- COP0 subset (Status/Cause/EPC/Count/Compare); others loud.
- RSP vector core deferred; `submit_task` is loud until then.
- JIT-ready: `CpuBackend` trait (`step`, `run_quanta`).

## 5. GPU plan

- RDP command decode (2-word commands), VI state, software framebuffer.
- Fill/triangle writes real pixels; TextureRect pending (no sampler).
- wgpu encoder + RSP ucodes at M5+.

## 6. Loader/boot plan

- Normalize z64/n64/v64 -> BE, parse 64-byte header, never claims boot.
- Boot maps ROM into RDRAM mirrors, sets pc=entry, steps with trace.
- Saves identified by size; ROMs never enter git (`$N64_ROM_FILE` gate).

## 7. Milestone acceptance

- M1: `cargo build` + `cargo test` clean, CLI info/step works on a test ROM.
- M2: more integer ops + FPU + desktop UI.
- Nothing marked done without evidence.
