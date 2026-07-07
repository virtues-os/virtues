#!/usr/bin/env python3
"""
Real-data inference test for the V1 retrieval stack on the Jetson.

Runs the FULL production-shaped pipeline against a real document (the 2026
papal encyclical, our standing test corpus):

  1. chunk the text (~380 words, 15% overlap — matches indexer.rs)
  2. embed every chunk with EmbeddingGemma  (CPU, --pooling mean, the prod
     prompts + Matryoshka-256 truncation) → throughput numbers
  3. embed a query, cosine-rank → top-30 candidates  (the retrieve step)
  4. rerank those 30 with gte-reranker-modernbert (GPU) → latency p50/p95
  5. print top-5 retrieved vs top-5 reranked so you can eyeball quality

Faithful to production (see virtues-core/src/search/{embedder,reranker}.rs):
  embed   = --pooling mean, prompts below, 768→256 Matryoshka + renorm, CPU
  rerank  = --pooling rank, cross-encoder over (query, doc), GPU

Usage (on the box):
  python3 encyclical_test.py \
    --text ~/encyclical.txt \
    --query "What does the encyclical say about care for the poor?" \
    --embed-gguf ~/bench-models/embeddinggemma-300m-qat-Q8_0.gguf \
    --rerank-gguf ~/bench-models/gte-reranker-modernbert-base-Q8_0.gguf
"""
import argparse, json, math, os, signal, subprocess, time, urllib.request

BIN = os.environ.get("BIN", "/usr/local/bin/llama-server")
# EmbeddingGemma's asymmetric prompts (model card / sentence-transformers).
DOC_PREFIX   = "title: none | text: "
QUERY_PREFIX = "task: search result | query: "
EMBED_DIM = 256  # production Matryoshka truncation


def post(url, payload, timeout=900):
    data = json.dumps(payload).encode()
    req = urllib.request.Request(url, data=data, headers={"content-type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read())


def tail(path, n=1800):
    try: return open(path).read()[-n:]
    except Exception: return "(no log)"


def start(gguf, port, mode, ngl):
    log = f"/tmp/enc_{mode}.log"
    flags = (["--embedding", "--pooling", "mean"] if mode == "embed"
             else ["--rerank", "--pooling", "rank"])
    p = subprocess.Popen(
        [BIN, *flags, "-m", gguf, "--host", "127.0.0.1", "--port", str(port),
         "-c", "8192", "-b", "8192", "-ub", "8192", "-np", "1", "--cache-ram", "0",
         "-ngl", str(ngl)],
        stdout=open(log, "w"), stderr=subprocess.STDOUT)
    for _ in range(120):
        if p.poll() is not None:
            raise SystemExit(f"{mode} sidecar died on startup:\n{tail(log)}")
        try:
            urllib.request.urlopen(f"http://127.0.0.1:{port}/health", timeout=2)
            if ngl != 0:
                off = "offloaded" in open(log).read().lower()
                print(f"  {mode} sidecar up (GPU offload: {'yes' if off else 'check log'})")
            else:
                print(f"  {mode} sidecar up (CPU)")
            return p, log
        except Exception:
            time.sleep(1)
    raise SystemExit(f"{mode} sidecar never healthy:\n{tail(log)}")


def stop(p):
    p.send_signal(signal.SIGTERM)
    try: p.wait(timeout=15)
    except Exception: p.kill()


def chunk(text, words=380, overlap=0.15):
    w = text.split()
    step = max(1, int(words * (1 - overlap)))
    out = []
    for i in range(0, len(w), step):
        c = " ".join(w[i:i + words]).strip()
        if c: out.append(c)
        if i + words >= len(w): break
    return out


def mrl256(v):
    v = v[:EMBED_DIM]
    n = math.sqrt(sum(x * x for x in v)) or 1.0
    return [x / n for x in v]


def embed(port, texts, doc=True):
    pref = DOC_PREFIX if doc else QUERY_PREFIX
    body = post(f"http://127.0.0.1:{port}/v1/embeddings", {"input": [pref + t for t in texts]})
    return [mrl256(d["embedding"]) for d in body["data"]], body.get("usage", {})


def cos(a, b):  # both already L2-normalized
    return sum(x * y for x, y in zip(a, b))


def pctl(xs, p):
    xs = sorted(xs); k = (len(xs) - 1) * p; lo = int(k); hi = min(lo + 1, len(xs) - 1)
    return xs[lo] + (xs[hi] - xs[lo]) * (k - lo)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--text", default=os.path.expanduser("~/encyclical.txt"))
    ap.add_argument("--query", required=True)
    ap.add_argument("--embed-gguf", default=os.path.expanduser("~/bench-models/embeddinggemma-300m-qat-Q8_0.gguf"))
    ap.add_argument("--rerank-gguf", default=os.path.expanduser("~/bench-models/gte-reranker-modernbert-base-Q8_0.gguf"))
    ap.add_argument("--embed-ngl", type=int, default=0)    # prod = CPU
    ap.add_argument("--rerank-ngl", type=int, default=99)  # prod = GPU
    ap.add_argument("--embed-batch", type=int, default=8)
    ap.add_argument("--k", type=int, default=30)
    ap.add_argument("--reps", type=int, default=20)
    args = ap.parse_args()

    text = open(os.path.expanduser(args.text), encoding="utf-8").read()
    chunks = chunk(text)
    print(f"=== corpus: {os.path.basename(args.text)} → {len(chunks)} chunks (~380w/15% overlap) ===")
    print(f"=== query: {args.query!r} ===")

    # ---- EMBED (throughput) ----
    print("\n[1] EmbeddingGemma — embed every chunk (CPU)" if args.embed_ngl == 0
          else "\n[1] EmbeddingGemma — embed every chunk (GPU)")
    ep, _ = start(args.embed_gguf, 28181, "embed", args.embed_ngl)
    try:
        embed(28181, chunks[:2])  # warm
        t0 = time.perf_counter()
        vecs = []
        for i in range(0, len(chunks), args.embed_batch):
            v, _ = embed(28181, chunks[i:i + args.embed_batch], doc=True)
            vecs.extend(v)
        wall = time.perf_counter() - t0
        qv, _ = embed(28181, [args.query], doc=False)
        qv = qv[0]
        print(f"  embedded {len(chunks)} chunks in {wall:.2f}s  ({len(chunks)/wall:.1f} chunks/s)")
        print(f"  (a ~100-page doc ≈ 130 chunks → ~{130/(len(chunks)/wall):.1f}s)")
    finally:
        stop(ep)

    # ---- RETRIEVE (cosine top-k) ----
    ranked = sorted(range(len(chunks)), key=lambda i: cos(qv, vecs[i]), reverse=True)
    topk = ranked[:args.k]
    print(f"\n[2] Retrieve — cosine top-{args.k}. Top-5 by embedding:")
    for r, i in enumerate(topk[:5], 1):
        print(f"  {r}. sim={cos(qv,vecs[i]):.3f}  {chunks[i][:90].strip()}…")

    # ---- RERANK (latency + quality) ----
    print(f"\n[3] gte-reranker-modernbert — rerank the {args.k} candidates (GPU)")
    rp, _ = start(args.rerank_gguf, 28182, "rerank", args.rerank_ngl)
    try:
        docs = [chunks[i] for i in topk]
        payload = {"query": args.query, "documents": docs, "top_n": args.k}
        url = "http://127.0.0.1:28182/v1/rerank"
        post(url, payload)  # warm
        times = []
        body = None
        for _ in range(args.reps):
            t0 = time.perf_counter(); body = post(url, payload); times.append((time.perf_counter()-t0)*1000)
        print(f"  rerank {args.k} docs: p50 {pctl(times,.5):.0f}ms  p95 {pctl(times,.95):.0f}ms  (vs ~300–500ms perceptibility)")
        results = sorted(body["results"], key=lambda r: r["relevance_score"], reverse=True)
        print("  Top-5 after rerank:")
        for r, item in enumerate(results[:5], 1):
            ci = topk[item["index"]]
            print(f"  {r}. score={item['relevance_score']:+.3f}  {chunks[ci][:90].strip()}…")
    finally:
        stop(rp)

    print("\nEyeball: do the reranked top-5 read as more on-topic than the embed top-5?")


if __name__ == "__main__":
    main()
