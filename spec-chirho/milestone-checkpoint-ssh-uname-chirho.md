<!-- For God so loved the world that he gave his only begotten Son, that whoever believes in him should not perish but have eternal life. -->

# Milestone Checkpoint: SSH uname -a Output Received

## Date: 2026-03-21

## Verified

```
ssh root@localhost -p 2222 'uname -a'
→ Lineluya lineluya 0.1.0 #1 SMP Lineluya 0.1.0 x86_64 GNU/Linux

ssh root@localhost -p 2222 'echo SSH_SUCCESS'
→ SSH_SUCCESS
```

## Known Architectural Debt (return after 1-2 more demo1 validations)

1. **GLOBAL_MM shared state** — VMA metadata, next_mmap_addr are global across processes. Per-task MmChirho ownership needed (Codex PRD workstream 3).
2. **Exec hybrid path** — restore_cow + mapper rebinding + shared MM fallback. Needs authoritative exec replacement where exec resets/rebuilds per-task MM.
3. **Boot PML4 staging** — exec still loads through boot PML4 then mirrors. Should load directly into per-process PT.
4. **PID-specific branches** — `pid >= 3`, `pid >= 4`, `pid == 3` in generic paths (scheduler, select, signal delivery).
5. **Finite-timeout select loop** — 50,000 HLT iterations with periodic yield hack. Should use timed wait_event.
6. **Pipe scan beyond nfds** — workaround for dropbear's nfds=1. Proper fix: typed fd dispatch or fix the connection fd passing.
7. **CloseWait force-exit** — session handler zombied from select loop. Proper fix: dropbear should detect EOF naturally.
8. **Session exit code** — SSH returns 124 (timeout) instead of 0. Session hangs after output relay.
9. **SLiRP connection reuse** — second SSH blocked by QEMU SLiRP (not kernel bug).
10. **Debug scaffolding** — AtomicU64 one-shot counters, PID-specific traces, TICK-TRACE.

## Next Steps (Codex-directed order)

1. ✅ Checkpoint this milestone (this file)
2. Run 1-2 more demo1 validations (ls /, id, date)
3. Return to per-task MM / exec cleanup hardening
