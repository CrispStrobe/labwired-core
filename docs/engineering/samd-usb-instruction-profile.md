# Current-source instruction-position diagnostic

A retained parent/candidate comparison attributed approximately nine additional
nRF52832 host instructions per step to generated code in unchanged peripheral
tick orchestration. Outlining optional USB construction did not restore the
unchanged full-gate result. This diagnostic collects instruction positions for
candidate `01e50371b9a7d341142399b84932b402fcac306b` before further source changes.

One hosted native build uses Rust 1.95.0 and the existing release/event-scheduler
configuration. The original nRF fixture is built once from its unchanged source;
its byte identity must match the admitted original. The current source executes
at the unchanged low/high step bounds with instruction positions and uncompressed
Callgrind position/name encoding. Cache and branch simulation remain disabled.
Original counts, client ELF mappings, binary, symbols, disassembly, tool versions
and losslessly compressed profiles are retained. No parent engine is reexecuted.

Independent readers must reconcile global self costs with total Ir, exclude
inclusive call edges, bind instruction addresses to the actual ELF, and preserve
negative deltas. Current-only instruction data identifies executed blocks; it
does not by itself establish a unique parent per-instruction delta. Use the
already retained parent disassembly/line costs, and state remaining ambiguity.
This is diagnostic evidence, not a source fix, replacement gate, baseline change
or physical timing result. The diagnostic branch is bounded separately from the
engine source change and does not select another full throughput campaign.
