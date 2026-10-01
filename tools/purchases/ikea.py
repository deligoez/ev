#!/usr/bin/env python3
"""IKEA Türkiye (ikea.com.tr) order history -> ev purchase NDJSON (spec/purchases.md §6).

Reads the raw export an agent saved under ~/.ev/purchases/ikea/ (outside every repository; the
raw orders carry addresses):

  raw/orders/<order>.json      one order: no, createdOn ("dd.mm.yyyy hh:mm"), status, and
                               subOrders[].orderDetails[], one per product as sold: sprCode,
                               familyName, functionName, catalogVariantValue, quantity,
                               totalPrice, status, categoryTree, productLink, articles[] (the
                               packages the product ships as)
  raw/invoices/index.json      per sub-order: the e-invoice saved, or why there is none
  raw/invoices/<order>-<sub>.html   the e-invoice page

The adapter reads the raw orders rather than catalog.json: the catalog has one row per
article, and a combination (a wardrobe made of a frame, doors and fittings) repeats the
combination's price on every article row. Here a combination is one line, priced once, keyed
by its SPR code; its articles are not lines of their own.

`paid` is the product's total price; delivery and assembly fees are per order and not spread
over the lines. The export has no delivery date. Cancelled orders and lines are dropped. The
only invoice is an HTML page; it is hung on every line of its sub-order. Usage:

  tools/purchases/ikea.py [--dir ~/.ev/purchases/ikea] | ev buy import --stdin
"""
import argparse
import json
import os
import re
import sys
from pathlib import Path

SOURCE = "ikea"

# Regexes searched in the lowercased product name; a storage box or a cup is kept, a bag or a
# candle is used up.
CONSUMABLE = [r"\bposet", r"\bpoşet", r"\bpeçete", r"\bmum\b", r"\bsabun\b", r"\bpil\b"]
CLOTHING = [r"\bterlik", r"\bçorap", r"\bönlük"]


def tr_lower(s: str) -> str:
    return (s or "").replace("İ", "i").replace("I", "ı").lower()


def bucket(name: str) -> str:
    t = tr_lower(name)
    if any(re.search(p, t) for p in CLOTHING):
        return "clothing"
    if any(re.search(p, t) for p in CONSUMABLE):
        return "consumable"
    return "durable"


def iso(date: str | None) -> str | None:
    m = re.match(r"(\d{2})\.(\d{2})\.(\d{4})", date or "")
    return f"{m.group(3)}-{m.group(2)}-{m.group(1)}" if m else None


def status_of(text: str | None) -> str:
    t = tr_lower(text)
    if "iptal" in t or "cancel" in t:
        return "cancelled"
    if "iade" in t or "refund" in t:
        return "returned"
    return "delivered"


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--dir", default="~/.ev/purchases/ikea")
    args = ap.parse_args()
    root = Path(os.path.expanduser(args.dir))
    index_file = root / "raw" / "invoices" / "index.json"
    invoices = json.loads(index_file.read_text()) if index_file.exists() else []

    keys_by_sub: dict[tuple[str, str], list[str]] = {}
    seen: dict[tuple[str, str], int] = {}
    out = sys.stdout
    for raw in sorted((root / "raw" / "orders").glob("*.json")):
        order = json.loads(raw.read_text())
        number = str(order["no"])
        if order.get("isCancelled") or status_of(order.get("status")) == "cancelled":
            continue
        for sub in order.get("subOrders") or []:
            if sub.get("isCancelled"):
                continue
            for d in sub.get("orderDetails") or []:
                status = status_of(d.get("status") or sub.get("status"))
                if status == "cancelled":
                    continue
                spr = str(d["sprCode"])
                n = seen.get((number, spr), 0)
                seen[(number, spr)] = n + 1
                key = f"{number}:{spr}" if n == 0 else f"{number}:{spr}:{n + 1}"
                keys_by_sub.setdefault((number, str(sub.get("no"))), []).append(key)
                name = " ".join(x for x in (d.get("familyName"), d.get("functionName")) if x)
                if d.get("catalogVariantValue"):
                    name = f"{name}, {d['catalogVariantValue']}"
                total = d.get("totalPrice")
                rec = {
                    "type": "purchase",
                    "source": SOURCE,
                    "key": key,
                    "shop": "IKEA",
                    "merchant": "IKEA",
                    "order": number,
                    "product_url": d.get("productLink"),
                    "sku": spr,
                    "name": name or spr,
                    "brand": "IKEA",
                    "category": (d.get("categoryTree") or "").replace(";", " > ") or None,
                    "ordered_at": iso(sub.get("createdOn") or order.get("createdOn")),
                    "qty": d.get("quantity") or 1,
                    "paid": f"{total:.2f}" if total is not None else None,
                    "currency": "TRY",
                    "status": status,
                    "bucket": bucket(name),
                    "raw": str(raw),
                }
                out.write(json.dumps({k: v for k, v in rec.items() if v is not None}, ensure_ascii=False))
                out.write("\n")

    for inv in invoices:
        keys = keys_by_sub.get((str(inv.get("order")), str(inv.get("sub"))))
        path = root / inv["file"] if inv.get("file") else None
        if not keys or not path or not path.exists():
            continue
        doc = {
            "type": "document",
            "source": SOURCE,
            "kind": "invoice",
            "file": str(path),
            "purchases": keys,
            "issuer": "IKEA",
        }
        out.write(json.dumps(doc, ensure_ascii=False))
        out.write("\n")


if __name__ == "__main__":
    main()
