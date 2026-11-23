"""Writes a verify run's report.html: each feature with what its checks said,
then every check, with its steps, details, log and screenshots."""

import fnmatch
import html
import json
import os
import time

STYLE = """
:root { --bg: #fbfbfa; --fg: #1d1d1b; --weak: #6b6b66; --line: #e4e4e0; --card: #ffffff;
        --pass: #1f7a3a; --fail: #b42318; --skip: #8a6d00; --none: #8a8a85; }
@media (prefers-color-scheme: dark) {
  :root { --bg: #18181a; --fg: #e8e8e4; --weak: #9a9a94; --line: #2e2e31; --card: #212124;
          --pass: #5cc77d; --fail: #ff7a6e; --skip: #e3c15a; --none: #7a7a75; } }
body { background: var(--bg); color: var(--fg); font: 14px/1.5 system-ui, sans-serif;
       margin: 0 auto; max-width: 1180px; padding: 24px 16px 64px; }
h1 { font-size: 22px; margin: 0 0 4px; } h2 { font-size: 16px; margin: 32px 0 8px; }
.meta { color: var(--weak); margin-bottom: 16px; }
.totals span { margin-right: 16px; font-weight: 600; }
table { border-collapse: collapse; width: 100%; }
td, th { border-bottom: 1px solid var(--line); padding: 6px 8px; text-align: left; vertical-align: top; }
th { color: var(--weak); font-weight: 500; font-size: 12px; }
.pass { color: var(--pass); } .fail { color: var(--fail); } .skip { color: var(--skip); }
.none { color: var(--none); }
.status { font-weight: 600; white-space: nowrap; }
.checks { color: var(--weak); font-size: 12px; }
pre { white-space: pre-wrap; margin: 4px 0 0; font-size: 12px; color: var(--weak); }
details { margin: 4px 0; } summary { cursor: pointer; }
.shots { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 6px; }
.shots a { display: block; width: 220px; } .shots img { width: 100%; border: 1px solid var(--line); border-radius: 4px; }
.shots figcaption { font-size: 11px; color: var(--weak); overflow-wrap: anywhere; }
.steps li.bad { color: var(--fail); }
"""


def rows_of(run_dir):
    rows = {}
    path = os.path.join(run_dir, "results.jsonl")
    if os.path.exists(path):
        for line in open(path):
            row = json.loads(line)
            rows[row["check"]] = row
    return rows


def matching(rows, check):
    """The results a feature's check names: unit:harlo covers its files too,
    and window:* a pattern of scenarios."""
    if check in rows:
        return [rows[check]]
    found = [r for c, r in rows.items() if fnmatch.fnmatchcase(c, check)
             or c.startswith(check + ":")]
    return found


def rollup(results):
    if not results:
        return "none"
    statuses = {r["status"] for r in results}
    if "fail" in statuses:
        return "fail"
    if statuses == {"skip"}:
        return "skip"
    return "pass"


def word(status):
    return {"pass": "passed", "fail": "failed", "skip": "skipped", "none": "not run"}[status]


def write(run_dir, items):
    rows = rows_of(run_dir)
    meta = {}
    try:
        with open(os.path.join(run_dir, "meta.json")) as f:
            meta = json.load(f)
    except (OSError, ValueError):
        pass
    e = html.escape
    out = [f"<!doctype html><meta charset=utf-8><meta name=viewport content='width=device-width'>"
           f"<title>Harlo verify</title><style>{STYLE}</style>"]
    started = meta.get("started")
    when = time.strftime("%Y-%m-%d %H:%M", time.localtime(started)) if started else "?"
    minutes = round((meta.get("finished", started or 0) - (started or 0)) / 60, 1) if started else "?"
    out.append(f"<h1>Harlo verify</h1><div class=meta>{e(meta.get('mode', '?'))} "
               f"{e(' '.join(meta.get('args', [])))} · {when} · {minutes} min · "
               f"{e(meta.get('branch', '?'))} {e(meta.get('commit', '')[:8])}"
               f"{' (with uncommitted changes)' if meta.get('dirty') else ''}<br>{e(meta.get('plan', ''))}</div>")
    counts = {s: sum(1 for r in rows.values() if r["status"] == s) for s in ("pass", "fail", "skip")}
    out.append("<div class=totals>" + "".join(
        f"<span class={s}>{counts[s]} {word(s)}</span>" for s in ("pass", "fail", "skip")) + "</div>")

    # Features.
    out.append("<h2>Features</h2><table><tr><th>Feature</th><th>Status</th><th>Checks</th></tr>")
    for it in items:
        results = [r for c in it.get("checks", []) for r in matching(rows, c)]
        status = rollup(results)
        names = []
        for c in it.get("checks", []):
            found = matching(rows, c)
            s = rollup(found)
            names.append(f"<span class={s}>{e(c)}</span>")
        out.append(f"<tr><td>{e(it['title'])}</td><td class='status {status}'>{word(status)}</td>"
                   f"<td class=checks>{', '.join(names)}</td></tr>")
    out.append("</table>")

    # Checks, failures first.
    order = {"fail": 0, "skip": 1, "pass": 2}
    out.append("<h2>Checks</h2><table><tr><th>Check</th><th>Status</th><th>Time</th><th>What it said</th></tr>")
    for r in sorted(rows.values(), key=lambda r: (order.get(r["status"], 3), r["check"])):
        cell = []
        if r.get("detail"):
            cell.append(f"<pre>{e(r['detail'])}</pre>")
        if r.get("steps"):
            items_html = "".join(
                f"<li class={'bad' if not s['ok'] else ''}>{e(s['step'])}"
                + (f": {e(s['expect'])}" if s.get("expect") else "") + "</li>"
                for s in r["steps"])
            cell.append(f"<details><summary>Steps</summary><ol class=steps>{items_html}</ol></details>")
        if r.get("log"):
            cell.append(f"<a href='{e(r['log'])}'>log</a>")
        if r.get("shots"):
            figures = "".join(
                f"<a href='{e(s)}'><figure style='margin:0'><img loading=lazy src='{e(s)}' alt=''>"
                f"<figcaption>{e(os.path.basename(s))}</figcaption></figure></a>" for s in r["shots"])
            cell.append(f"<details{' open' if r['status'] == 'fail' else ''}><summary>"
                        f"{len(r['shots'])} screenshots</summary><div class=shots>{figures}</div></details>")
        seconds = f"{r['seconds']:g} s" if r.get("seconds") else ""
        out.append(f"<tr><td>{e(r['check'])}</td><td class='status {r['status']}'>{word(r['status'])}</td>"
                   f"<td>{seconds}</td><td>{''.join(cell)}</td></tr>")
    out.append("</table>")
    path = os.path.join(run_dir, "report.html")
    with open(path, "w") as f:
        f.write("\n".join(out))
    return path
