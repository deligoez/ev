#!/usr/bin/env python3
"""AliExpress order history -> ev purchase NDJSON (spec/purchases.md §6).

Reads the raw export an agent saved under ~/.ev/purchases/aliexpress/ (outside every
repository; the receipts carry names and addresses):

  catalog.json                 one object per order line: order_id, order_line_id, date,
                               product_title, product_id, sku_attributes, quantity, unit_price,
                               line_total, currency, seller, status (completed|expired),
                               product_url, order_detail_url
  raw/orders/<order>.json      the order detail; its orderEndTime is when the order was
                               completed (receipt confirmed by the buyer or automatically)
  raw/invoices/<order>-1.pdf   the order receipt AliExpress renders (no formal seller invoices
                               exist on the account)

`expired` orders were never paid and are dropped. `paid` is the line total (unit price x
quantity) in the order's currency (USD for older orders, TRY later); shipping, store
discounts, coins and import duty are per order and not spread over the lines.
`delivered_at` is the completion date, which is on or after the real delivery. Usage:

  tools/purchases/aliexpress.py [--dir ~/.ev/purchases/aliexpress] | ev buy import --stdin
"""
import argparse
import json
import os
import re
import sys
from datetime import datetime
from pathlib import Path

SOURCE = "aliexpress"

# Regexes searched in the lowercased title (English or Turkish, depending on the order's era).
CONSUMABLE = [
    r"\bbattery\b(?! for)", r"\bbatteries\b", r"\bfilter cartridge", r"\bink\b", r"\btoner\b",
    r"\bpil\b", r"\bkartuş",
]
KEEP = [r"\bdummy battery", r"\bcharger", r"\bşarj", r"\bholder", r"\bcase\b"]
CLOTHING = [
    r"\bt-shirt", r"\bjacket\b", r"\bsocks?\b", r"\bshoes?\b", r"\bpants\b", r"\bdress\b",
    r"\bçorap", r"\btişört", r"\bceket", r"\bayakkabı", r"\bmont\b",
]


def bucket(title: str) -> str:
    t = (title or "").replace("İ", "i").replace("I", "ı").lower()
    if any(re.search(p, t) for p in CLOTHING):
        return "clothing"
    if any(re.search(p, t) for p in CONSUMABLE) and not any(re.search(p, t) for p in KEEP):
        return "consumable"
    return "durable"


def completed_on(path: Path) -> str | None:
    """The order detail's orderEndTime ('May 7, 2024') as an ISO date."""
    if not path.exists():
        return None
    m = re.search(r'"orderEndTime"\s*:\s*"([^"]+)"', path.read_text())
    if not m:
        return None
    try:
        return datetime.strptime(m.group(1).strip(), "%b %d, %Y").date().isoformat()
    except ValueError:
        return None


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--dir", default="~/.ev/purchases/aliexpress")
    args = ap.parse_args()
    root = Path(os.path.expanduser(args.dir))
    catalog = json.loads((root / "catalog.json").read_text())

    keys_by_order: dict[str, list[str]] = {}
    merchants: dict[str, set[str]] = {}
    seen: dict[tuple[str, str], int] = {}
    out = sys.stdout
    for line in catalog:
        if line.get("status") != "completed":
            continue
        order, line_id = line["order_id"], line["order_line_id"]
        n = seen.get((order, line_id), 0)
        seen[(order, line_id)] = n + 1
        key = f"{order}:{line_id}" if n == 0 else f"{order}:{line_id}:{n + 1}"
        keys_by_order.setdefault(order, []).append(key)
        if line.get("seller"):
            merchants.setdefault(order, set()).add(line["seller"])
        name = line["product_title"]
        attrs = [
            f"{a.get('name') or a.get('propertyName')}: {a.get('value') or a.get('propertyValue')}"
            for a in line.get("sku_attributes") or []
            if isinstance(a, dict)
        ]
        if attrs:
            name = f"{name} ({', '.join(attrs)})"
        raw = root / "raw" / "orders" / f"{order}.json"
        total = line.get("line_total")
        rec = {
            "type": "purchase",
            "source": SOURCE,
            "key": key,
            "shop": "AliExpress",
            "merchant": line.get("seller"),
            "order": order,
            "order_url": line.get("order_detail_url"),
            "product_url": line.get("product_url"),
            "sku": line.get("product_id"),
            "name": name,
            "ordered_at": line.get("date"),
            "delivered_at": completed_on(raw),
            "qty": line.get("quantity") or 1,
            "paid": f"{total:.2f}" if total is not None else None,
            "currency": line.get("currency"),
            "status": "delivered",
            "bucket": bucket(line["product_title"]),
            "raw": str(raw) if raw.exists() else None,
        }
        out.write(json.dumps({k: v for k, v in rec.items() if v is not None}, ensure_ascii=False))
        out.write("\n")

    for pdf in sorted((root / "raw" / "invoices").glob("*.pdf")):
        order = pdf.stem.split("-")[0]
        if order not in keys_by_order:
            continue
        ms = merchants.get(order, set())
        doc = {
            "type": "document",
            "source": SOURCE,
            "kind": "invoice",
            "file": str(pdf),
            "purchases": keys_by_order[order],
            "issuer": next(iter(ms)) if len(ms) == 1 else None,
            "note": "AliExpress order receipt",
        }
        out.write(json.dumps({k: v for k, v in doc.items() if v is not None}, ensure_ascii=False))
        out.write("\n")


if __name__ == "__main__":
    main()
