# TODO board (n64-rs)

Statuses: done (verified), active (in progress), next (queued), later.

## Entire N64 emulation total: ~1 out of 100%

100% = every game made originally for the Nintendo 64 boots and runs.
Honest breakdown:
- CPU integer 15% · FPU 0% · COP0/TLB 5% · RSP 0%
- RDP 5% · audio 5% · saves 20% · PIF/boot 10%
- Frontend 20% · per-game compat 0%

## Done (verified by tests)

- [x] Workspace (12 crates), VR4300 integer core with delay/load-delay
- [x] RDRAM + KSEG mirrors, SP DMEM/IMEM, MMIO guard
- [x] Cart ROM + PIF ROM windows, read-only with KSEG1 mirrors
- [x] z64/n64/v64 normalize + header parse
- [x] PIF boot: LLE reset vector when PIF ROM present, HLE IPL3 copy
      otherwise; CIC checksum detection (unknown CIC is loud)
- [x] Unaligned LWL/LWR/SWL/SWR, LL/SC, SYNC for IPL3 boot code
- [x] RSP scalar core (DMEM-relative loads/stores, BREAK halt, vector loud)
- [x] RDP decode + software fill validation
- [x] Saves sized by hardware truth, loopback net harness, CLI

## Active

- [ ] Retail frontier: first SM64 (`$N64_ROM_FILE`) boot fault out of
      `retail_rom_steps` — IPL3 execution from the HLE handoff

## Next (retail-driven)

- [ ] FPU (COP1), more COP0, TLB
- [ ] RSP vector ops on demand, RDP sampler
- [ ] PIF boot sequence, VI timing, audio tasks

## Later

- [ ] Desktop UI, wgpu backend, JIT
- [ ] Fully done emulator OR last verified retail step
