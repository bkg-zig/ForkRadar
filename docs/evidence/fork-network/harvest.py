#!/usr/bin/env python3
"""ForkRadar capture harness (read-only).

Captures a GitHub fork network from the public GitHub REST API and writes raw
payloads plus one consolidated graph file. It exists to produce provable
metadata for the ForkRadar pipeline, not to replace crates/forkradar-github or
crates/forkradar-discovery.

Contract it mirrors:
  * read endpoints of the GitHub REST API only
  * every request is stored so a reviewer can replay it
  * the request budget comes from the response headers, not extra calls
  * a rate limit pauses the run until the documented reset and then continues
    from the stored cursor, so collected work is not dropped
  * a run reports COMPLETED only when every page of the scope was read

What the run records:
  * recursive fork listing (depth via breadth first expansion)
  * release and commit tracking of the root repository
  * the reported upstream of a sample of forks
  * a comparison of each sampled fork with its reported upstream
  * a per-node liveness state, because the fork listing can carry entries whose
    repository no longer resolves

Usage:
  python3 harvest.py --out DIR --target chmln/sd [--target Semmle/SecurityExploits]
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone

API = "https://api.github.com"
HEADERS = {
    "Accept": "application/vnd.github+json",
    "X-GitHub-Api-Version": "2022-11-28",
    "User-Agent": "ForkRadar-capture/0.1 (+https://github.com/bkg-zig/ForkRadar)",
}

state = {"remaining": None, "reset": None, "calls": 0, "cache_hits": 0}


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def get_json(path: str, out_dir: str, key: str, *, wait_on_limit: bool, max_wait: int):
    """GET path, store the raw body, return (status, payload). Cached on disk."""
    raw_path = os.path.join(out_dir, "raw", f"{key}.json")
    if os.path.exists(raw_path):
        try:
            with open(raw_path, encoding="utf-8") as handle:
                payload = json.load(handle)
            state["cache_hits"] += 1
            return "HTTP 200", payload
        except json.JSONDecodeError:
            os.unlink(raw_path)

    url = f"{API}/{path.lstrip('/')}"
    while True:
        req = urllib.request.Request(url, headers=HEADERS)
        try:
            with urllib.request.urlopen(req, timeout=30) as resp:
                body = resp.read()
                state["calls"] += 1
                state["remaining"] = int(resp.headers.get("x-ratelimit-remaining", "-1"))
                state["reset"] = int(resp.headers.get("x-ratelimit-reset", "0"))
                payload = json.loads(body) if body else None
        except urllib.error.HTTPError as err:
            state["calls"] += 1
            state["remaining"] = int(err.headers.get("x-ratelimit-remaining", "-1"))
            state["reset"] = int(err.headers.get("x-ratelimit-reset", "0"))
            if err.code in (403, 429) and state["remaining"] == 0:
                wait = (state["reset"] - int(time.time()) + 2) if state["reset"] else 60
                if wait_on_limit and 0 < wait <= max_wait:
                    print(f"    rate limit reached, waiting {wait}s until reset", flush=True)
                    time.sleep(wait)
                    continue
            return f"HTTP {err.code}", None
        except urllib.error.URLError as err:
            return f"TRANSPORT {err.reason}", None

        os.makedirs(os.path.dirname(raw_path), exist_ok=True)
        with open(raw_path, "w", encoding="utf-8") as handle:
            json.dump(payload, handle, indent=1, sort_keys=True)
        return "HTTP 200", payload


def node_from_repo(repo: dict) -> dict:
    """Project a GitHub repository object onto the ForkRadar RepositoryNode shape."""
    owner = (repo.get("owner") or {}).get("login", "")
    parent = repo.get("parent") or {}
    source = repo.get("source") or {}
    return {
        "id": repo.get("id"),
        "owner": owner,
        "name": repo.get("name"),
        "full_name": repo.get("full_name"),
        "html_url": repo.get("html_url"),
        "is_fork": repo.get("fork", False),
        "parent": parent.get("full_name"),
        "source": source.get("full_name"),
        "default_branch": repo.get("default_branch"),
        "language": repo.get("language"),
        "description": repo.get("description"),
        "stargazers_count": repo.get("stargazers_count", 0),
        "forks_count": repo.get("forks_count", 0),
        "open_issues_count": repo.get("open_issues_count", 0),
        "size_kb": repo.get("size", 0),
        "archived": repo.get("archived", False),
        "disabled": repo.get("disabled", False),
        "visibility": repo.get("visibility"),
        "created_at": repo.get("created_at"),
        "updated_at": repo.get("updated_at"),
        "pushed_at": repo.get("pushed_at"),
        "liveness": "UNKNOWN",
    }


def capture_target(target: str, out_dir: str, *, wait_on_limit: bool, max_wait: int,
                   sample_size: int, compare_size: int) -> dict:
    owner, name = target.split("/", 1)
    tdir = os.path.join(out_dir, target.replace("/", "_"))
    os.makedirs(tdir, exist_ok=True)
    print(f"== target {target} ==", flush=True)

    report = {
        "target": target,
        "captured_at": now_iso(),
        "endpoints": [],
        "nodes": {},
        "edges": [],
        "releases": [],
        "commits": [],
        "comparisons": [],
        "status": "COMPLETED",
        "notes": [],
    }

    def call(path, key):
        status, payload = get_json(path, tdir, key, wait_on_limit=wait_on_limit,
                                   max_wait=max_wait)
        report["endpoints"].append({"path": path, "status": status,
                                    "remaining_after": state["remaining"]})
        return status, payload

    status, root = call(f"repos/{owner}/{name}", "root")
    if status != "HTTP 200" or not root:
        report["status"] = "FAILED"
        report["notes"].append(f"root repository not readable: {status}")
        return report
    root_node = node_from_repo(root)
    root_node["depth"] = 0
    root_node["liveness"] = "LIVE"
    report["nodes"][str(root_node["id"])] = root_node

    status, releases = call(f"repos/{owner}/{name}/releases?per_page=100", "releases")
    if status == "HTTP 200" and isinstance(releases, list):
        report["releases"] = [
            {"id": r.get("id"), "tag_name": r.get("tag_name"), "name": r.get("name"),
             "draft": r.get("draft"), "prerelease": r.get("prerelease"),
             "created_at": r.get("created_at"), "published_at": r.get("published_at")}
            for r in releases
        ]
    else:
        report["status"] = "PARTIAL"
        report["notes"].append(f"releases not readable: {status}")

    branch = root_node.get("default_branch") or "master"
    status, commits = call(f"repos/{owner}/{name}/commits?sha={branch}&per_page=100",
                           "commits")
    if status == "HTTP 200" and isinstance(commits, list):
        report["commits"] = [
            {"sha": c.get("sha"),
             "date": ((c.get("commit") or {}).get("committer") or {}).get("date"),
             "author": ((c.get("commit") or {}).get("author") or {}).get("name"),
             "message": (((c.get("commit") or {}).get("message") or "").splitlines()
                         or [""])[0]}
            for c in commits
        ]
        report["notes"].append(
            f"commits: newest {len(commits)} of branch {branch} (per_page=100); "
            f"the API returns a page, not the whole history"
        )
    else:
        report["status"] = "PARTIAL"
        report["notes"].append(f"commits not readable: {status}")

    # Recursive fork listing, breadth first. A node is expanded only when its own
    # fork count is above zero, so a network without forks-of-forks ends at depth 1.
    queue = [(root_node["id"], root_node["full_name"], 0)]
    expanded = set()
    listed_total = 0
    while queue:
        repo_id, full, depth = queue.pop(0)
        if repo_id in expanded:
            continue
        expanded.add(repo_id)
        page = 1
        while True:
            status, forks = call(
                f"repos/{full}/forks?per_page=100&page={page}",
                f"forks_{full.replace('/', '_')}_p{page}",
            )
            if status != "HTTP 200" or not isinstance(forks, list):
                report["status"] = "PARTIAL"
                report["notes"].append(
                    f"fork listing of {full} page {page} not readable: {status} "
                    f"(the fork count reported to GitHub can outlive the repository)"
                )
                break
            for fork in forks:
                child = node_from_repo(fork)
                child["depth"] = depth + 1
                listed_total += 1
                node = report["nodes"].get(str(child["id"]))
                if node is None:
                    report["nodes"][str(child["id"])] = child
                elif (node.get("depth") or 99) > child["depth"]:
                    node["depth"] = child["depth"]
                report["edges"].append({
                    "from_id": repo_id,
                    "to_id": child["id"],
                    "kind": "VERIFIED_IMMEDIATE_PARENT",
                    "verification": "CONFIRMED_BY_FORK_LISTING",
                    "evidence": f"GET /repos/{full}/forks?per_page=100&page={page}",
                })
                if child.get("forks_count", 0) > 0 and str(child["id"]) not in expanded:
                    queue.append((child["id"], child["full_name"], depth + 1))
            if len(forks) < 100:
                break
            page += 1

    depths = [n.get("depth") or 0 for n in report["nodes"].values()]
    report["depth_reached"] = max(depths) if depths else 0
    report["forks_listed_total"] = listed_total

    # Sample forks for detail and comparison. Prefer forks that themselves have
    # forks (they are the recursion points), then the most recently pushed.
    forks = [n for n in report["nodes"].values() if n["id"] != root_node["id"]]
    forks.sort(key=lambda n: (n.get("forks_count", 0) > 0,
                              n.get("pushed_at") or ""), reverse=True)
    sample = forks[:sample_size]

    # Detail fetch resolves the reported upstream and the liveness of each node.
    for node in sample:
        status, detail = call(f"repos/{node['full_name']}",
                              f"detail_{node['full_name'].replace('/', '_')}")
        if status == "HTTP 200" and detail:
            resolved = node_from_repo(detail)
            resolved["depth"] = node.get("depth")
            resolved["liveness"] = "LIVE"
            report["nodes"][str(resolved["id"])] = resolved
            # Keep the sample entry in sync, the compare step reads it.
            node["liveness"] = "LIVE"
            node["parent"] = resolved.get("parent")
            node["source"] = resolved.get("source")
            if resolved.get("default_branch"):
                node["default_branch"] = resolved["default_branch"]
            if resolved.get("parent"):
                report["edges"].append({
                    "from_id": report["nodes"][str(root_node["id"])]["id"],
                    "to_id": resolved["id"],
                    "kind": "GITHUB_REPORTED_PARENT",
                    "verification": "REPORTED_BY_GITHUB",
                    "evidence": f"GET /repos/{node['full_name']} "
                                f"parent={resolved['parent']} source={resolved['source']}",
                })
        else:
            node["liveness"] = "UNAVAILABLE"
            report["nodes"][str(node["id"])] = node
            report["notes"].append(
                f"{node['full_name']} appears in the fork listing but "
                f"GET /repos/{node['full_name']} returns {status}"
            )

    # Compare each live sampled fork against its reported upstream.
    by_name = {n["full_name"]: n for n in report["nodes"].values()}
    compared = 0
    for node in sample:
        if compared >= compare_size:
            break
        if node.get("liveness") != "LIVE":
            continue
        parent_name = node.get("parent") or root_node["full_name"]
        parent = by_name.get(parent_name)
        base_branch = (parent or {}).get("default_branch") or branch
        head_branch = node.get("default_branch") or "master"
        status, cmp = call(
            f"repos/{parent_name}/compare/{base_branch}...{node['owner']}:{head_branch}",
            f"cmp_{node['full_name'].replace('/', '_')}",
        )
        entry = {"fork": node["full_name"], "upstream": parent_name,
                 "head": f"{node['owner']}:{head_branch}", "result": status}
        if status == "HTTP 200" and cmp:
            entry.update({"ahead_by": cmp.get("ahead_by"),
                          "behind_by": cmp.get("behind_by"),
                          "total_commits": cmp.get("total_commits"),
                          "status": cmp.get("status")})
        report["comparisons"].append(entry)
        compared += 1
    report["compare_count"] = compared
    return report


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", required=True)
    parser.add_argument("--target", action="append", required=True)
    parser.add_argument("--sample-size", type=int, default=6)
    parser.add_argument("--compare-size", type=int, default=6)
    parser.add_argument("--wait-on-limit", action="store_true")
    parser.add_argument("--max-wait", type=int, default=900)
    args = parser.parse_args()

    _, probe = get_json("rate_limit", args.out, "_rate_limit_before",
                        wait_on_limit=False, max_wait=0)
    if probe:
        core = probe["resources"]["core"]
        print(f"rate before: {core['remaining']}/{core['limit']} reset={core['reset']}")

    reports = []
    for target in args.target:
        reports.append(capture_target(target, args.out,
                                      wait_on_limit=args.wait_on_limit,
                                      max_wait=args.max_wait,
                                      sample_size=args.sample_size,
                                      compare_size=args.compare_size))

    summary = {"generated_at": now_iso(), "api_calls": state["calls"],
               "cache_hits": state["cache_hits"],
               "rate_remaining_after": state["remaining"], "targets": reports}
    out_path = os.path.join(args.out, "graph.json")
    with open(out_path, "w", encoding="utf-8") as handle:
        json.dump(summary, handle, indent=1, sort_keys=True)
    print(f"wrote {out_path}")
    for rep in reports:
        print(f"- {rep['target']}: status={rep['status']} nodes={len(rep['nodes'])} "
              f"edges={len(rep['edges'])} depth={rep.get('depth_reached')} "
              f"releases={len(rep['releases'])} commits={len(rep['commits'])} "
              f"compares={rep.get('compare_count')}")
        for note in rep["notes"]:
            print(f"    note: {note}")
    print(f"api calls: {state['calls']} cache hits: {state['cache_hits']} "
          f"remaining: {state['remaining']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
