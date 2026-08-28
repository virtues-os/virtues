#!/usr/bin/env python3
"""
Real-document embed bench — chunk a text file the way the indexer does and
time how long the embedding sidecar takes to make it searchable.

The "user drops a big document and waits" scenario. Point it at the embed
sidecar (live GPU service :18181, or a hand-started CPU instance).

stdlib only.

  python3 embed_doc_bench.py --file encyclical.txt --label gpu
  python3 embed_doc_bench.py --file encyclical.txt --label cpu --embed-url http://127.0.0.1:28181
"""
import argparse, json, time, urllib.request, urllib.error

def chunk_words(text, words_per_chunk):
    w = text.split()
    return [" ".join(w[i:i + words_per_chunk]) for i in range(0, len(w), words_per_chunk)]

def post(url, payload, timeout=600):
    data = json.dumps(payload).encode()
    req = urllib.request.Request(url, data=data, headers={"content-type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read())

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--file", required=True)
    ap.add_argument("--label", default="run")
    ap.add_argument("--embed-url", default="http://127.0.0.1:18181")
    ap.add_argument("--words-per-chunk", type=int, default=380)  # ~512 tokens
    ap.add_argument("--batch", type=int, default=16)
    ap.add_argument("--repeat", type=int, default=1)             # loop the doc to scale up
    args = ap.parse_args()

    text = open(args.file, encoding="utf-8").read()
    chunks = chunk_words(text, args.words_per_chunk) * args.repeat
    url = f"{args.embed_url}/v1/embeddings"

    print(f"=== embed-doc: {args.label} ===")
    print(f"file {args.file}  | chunks {len(chunks)}  | ~{args.words_per_chunk} words/chunk  | batch {args.batch}")

    # warmup
    try:
        post(url, {"input": chunks[:2]})
    except urllib.error.URLError as e:
        print(f"ERROR: embed sidecar unreachable at {url} — {e}")
        return

    t0 = time.perf_counter()
    total_tokens = 0
    for i in range(0, len(chunks), args.batch):
        body = post(url, {"input": chunks[i:i + args.batch]})
        total_tokens += body.get("usage", {}).get("prompt_tokens", 0)
    wall = time.perf_counter() - t0

    print(f"  wall-clock     {wall:.2f} s")
    print(f"  chunks/sec     {len(chunks) / wall:.1f}")
    if total_tokens:
        print(f"  tokens         {total_tokens}")
        print(f"  tokens/sec     {total_tokens / wall:.0f}")
    print(f"\n--- paste-ready ---\n{args.label:<10} {len(chunks)} chunks ({total_tokens} tok) in {wall:.2f}s  ({total_tokens/wall:.0f} tok/s)")

if __name__ == "__main__":
    main()
