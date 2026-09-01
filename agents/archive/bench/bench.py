#!/usr/bin/env python3
"""
Virtues inference bench — pure measurement client for the llama-server sidecars.

Measures the two user-facing moments (see agents/archive/inference-bench-spec.md):
  TEST A  live rerank latency (30 docs, p50/p95) at short/realistic/long lengths
  TEST B  100-page-PDF embed wall-clock (passages/sec, tokens/sec)

It does NOT start or configure llama-server — point it at whatever is listening.
Run it once per backend (e.g. CPU services on :18181/:18182, then a hand-started
GPU instance on :28181/:28182) and label each run.

stdlib only. Copy to the box, run with system python3.

Examples
  python3 bench.py --label jetson-cpu
  python3 bench.py --label jetson-gpu --embed-url http://127.0.0.1:28181 --rerank-url http://127.0.0.1:28182
"""
import argparse, json, random, statistics, time, urllib.request, urllib.error

WORDS = ("memory search vector postgres reranker embedding latency jetson orin "
         "cuda offload context window passage document query relevance score "
         "appliance sidecar inference throughput batch token model precision "
         "narrative person place note chat space cosine hnsw index pipeline").split()

def synth(n_words, seed):
    r = random.Random(seed)
    return " ".join(r.choice(WORDS) for _ in range(n_words))

def post(url, payload, timeout=300):
    data = json.dumps(payload).encode()
    req = urllib.request.Request(url, data=data, headers={"content-type": "application/json"})
    t0 = time.perf_counter()
    with urllib.request.urlopen(req, timeout=timeout) as r:
        body = json.loads(r.read())
    return time.perf_counter() - t0, body

def pctl(xs, p):
    xs = sorted(xs)
    if not xs: return float("nan")
    k = (len(xs) - 1) * p
    lo = int(k)
    hi = min(lo + 1, len(xs) - 1)
    return xs[lo] + (xs[hi] - xs[lo]) * (k - lo)

def test_a(rerank_url, reps, warmup, ndocs=30):
    print(f"\nTEST A — live rerank ({ndocs} docs), p50/p95 in ms")
    print(f"  {'length':<12}{'p50':>8}{'p95':>8}{'max':>8}{'tok':>8}")
    url = f"{rerank_url}/v1/rerank"
    tiers = [("short~80tok", 60), ("realistic~250tok", 190), ("long~800tok", 600)]
    rows = []
    query = "what did I decide about the database and vector search"
    for name, w in tiers:
        docs = [synth(w, seed=1000 + i) for i in range(ndocs)]
        payload = {"query": query, "documents": docs, "top_n": ndocs}
        for _ in range(warmup):
            post(url, payload)
        times, toks = [], 0
        for _ in range(reps):
            dt, body = post(url, payload)
            times.append(dt * 1000)
            toks = body.get("usage", {}).get("total_tokens", toks)
        p50, p95, mx = pctl(times, .5), pctl(times, .95), max(times)
        print(f"  {name:<12}{p50:>8.1f}{p95:>8.1f}{mx:>8.1f}{toks:>8}")
        rows.append((name, p50, p95))
    return rows

def test_b(embed_url, pages, words_per_passage, passages_per_page, batch):
    print("\nTEST B — 100-page PDF embed")
    url = f"{embed_url}/v1/embeddings"
    n = pages * passages_per_page
    passages = [synth(words_per_passage, seed=5000 + i) for i in range(n)]
    # warmup one small batch (kernel/model warm)
    post(url, {"input": passages[:2]})
    t0 = time.perf_counter()
    total_tokens = 0
    for i in range(0, n, batch):
        _, body = post(url, {"input": passages[i:i + batch]})
        total_tokens += body.get("usage", {}).get("prompt_tokens", 0)
    wall = time.perf_counter() - t0
    print(f"  passages         {n}")
    print(f"  wall-clock       {wall:.2f} s")
    print(f"  passages/sec     {n / wall:.1f}")
    if total_tokens:
        print(f"  tokens/sec       {total_tokens / wall:.0f}  (server-reported)")
    return wall

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--label", default="run")
    ap.add_argument("--embed-url", default="http://127.0.0.1:18181")
    ap.add_argument("--rerank-url", default="http://127.0.0.1:18182")
    ap.add_argument("--reps", type=int, default=30)
    ap.add_argument("--docs", type=int, default=30)
    ap.add_argument("--warmup", type=int, default=3)
    ap.add_argument("--pages", type=int, default=100)
    ap.add_argument("--passages-per-page", type=int, default=1)   # ~1 chunk/page ≈ 512 tok
    ap.add_argument("--words-per-passage", type=int, default=380) # ≈512 tokens
    ap.add_argument("--embed-batch", type=int, default=16)
    ap.add_argument("--skip-a", action="store_true")
    ap.add_argument("--skip-b", action="store_true")
    args = ap.parse_args()

    print(f"=== bench: {args.label} ===")
    print(f"rerank {args.rerank_url}   embed {args.embed_url}   reps {args.reps}")
    a = b = None
    try:
        if not args.skip_a: a = test_a(args.rerank_url, args.reps, args.warmup, args.docs)
        if not args.skip_b:
            b = test_b(args.embed_url, args.pages, args.words_per_passage,
                       args.passages_per_page, args.embed_batch)
    except urllib.error.URLError as e:
        print(f"\nERROR talking to a sidecar: {e}\nIs llama-server listening on those ports?")
        return

    print("\n--- paste-ready ---")
    if a:
        real = next((r for r in a if r[0].startswith("realistic")), a[0])
        print(f"{args.label:<14} rerank30 realistic: p50 {real[1]:.0f}ms  p95 {real[2]:.0f}ms")
    if b is not None:
        print(f"{args.label:<14} 100-page embed: {b:.1f}s")

if __name__ == "__main__":
    main()
