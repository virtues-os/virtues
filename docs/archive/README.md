# Archive — design history

**Nothing in this directory describes the system as it is.** These are documents
whose subject was cut, replaced, or decided. They are kept because the reasoning
is still worth having — a cut feature that gets reconsidered should start from
the argument that killed it, not from a blank page — but they must never be read
as a description of current behavior, and they are not maintained.

If you are looking for how something works today, go back to
[`../README.md`](../README.md).

| Doc | Why it's here |
|---|---|
| [things.md](things.md) | The "Things" feature (foldered collections, pins, AI memos). Removed 2026-07-21: `wiki_things` and `wiki_thing_pins` dropped in migration `0060`, the `/api/things` write path and `api::things` module deleted. Projects and hobbies became notebooks; concepts became topics. |
| [stories-plan.md](stories-plan.md) | The claim-style story — a thesis whose body is a rendered, cited account gathered by the magnet. Cut from v1 2026-07-22 after a spike on real data could not establish who it helped. `wiki_stories` and `wiki_story_members` dropped in migration `0060`; `app_notebooks` is the surviving primitive. The durable technical findings (the magnet's dead centroid dimension, the reranker being ColBERT MaxSim rather than a cross-encoder) were carried forward into [`../ir-notes.md`](../ir-notes.md). |
| [inference-bench-spec.md](inference-bench-spec.md) | The plan for choosing between an NVIDIA Jetson and a Qualcomm Q6A as the appliance board. The bench was run and the question is answered — the Radxa Dragon Q6A is the one supported board. Measured results live in [`../npu-hardware-findings.md`](../npu-hardware-findings.md), which is still current. |
| [bench/](bench) | The Jetson-era benchmark harness that produced those numbers (`bench.py`, `embed_doc_bench.py`, `encyclical_test.py`, `gpu_embed_bench.sh`). Written against a board we no longer ship and a model pair we have since replaced; kept as a worked example of the measurement method, not as a runnable tool. |
