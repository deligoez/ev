#!/usr/bin/env python3
"""An invented shop's order history -> ev purchase NDJSON: the worked example of an adapter.

A real adapter reads what an agent saved from one shop (pages, JSON, invoices) and prints one
JSON object per line for `ev buy import --stdin`. This one reads `orders.json` next to it, an
invented export shaped like most shops' order pages:

  [{no, placed: "dd.mm.yyyy hh:mm", delivered, status, invoice,
    items: [{sku, title, brand, qty, total: "1.999,00 TL", url, warranty_months, images}]}]

What it shows, one each: a `purchase` line keyed so a re-import updates it, a `document` (the
invoice) hung on the lines of its order, a `link` (the product page), a `coverage` (the
shop's warranty), an `image` (a saved product picture: the adapter knows which picture is
which line, so it names it), a line dropped (a cancelled order) and a `bucket` guess (a bag is
used up). Paths are made absolute: ev copies the files when the line is linked and brought.
Usage:

  examples/purchases/example-shop.py [--dir examples/purchases] | ev buy import --stdin
"""
import argparse
import json
import re
import sys
from pathlib import Path

SOURCE = "example-shop"
CONSUMABLE = [r"poşet", r"peçete", r"\bpil\b"]


def iso(date):
    """`06.12.2024 14:05` -> `2024-12-06`."""
    m = re.match(r"(\d{2})\.(\d{2})\.(\d{4})", date or "")
    return f"{m.group(3)}-{m.group(2)}-{m.group(1)}" if m else None


def amount(text):
    """`1.999,00 TL` -> `1999.00`."""
    digits = re.sub(r"[^\d,]", "", text or "").replace(",", ".")
    return digits or None


def bucket(title):
    t = title.lower()
    return "consumable" if any(re.search(p, t) for p in CONSUMABLE) else "durable"


def lines(base):
    for order in json.loads((base / "orders.json").read_text(encoding="utf-8")):
        if order.get("status") == "İptal edildi":
            continue
        keys = []
        for item in order["items"]:
            key = f"{order['no']}:{item['sku']}"
            keys.append(key)
            yield {
                "type": "purchase",
                "source": SOURCE,
                "key": key,
                "name": item["title"],
                "shop": "Example Shop",
                "order": order["no"],
                "sku": item["sku"],
                "brand": item.get("brand"),
                "ordered_at": iso(order.get("placed")),
                "delivered_at": iso(order.get("delivered")),
                "qty": item.get("qty", 1),
                "paid": amount(item.get("total")),
                "currency": "TRY",
                "bucket": bucket(item["title"]),
            }
            if item.get("url"):
                yield {"type": "link", "source": SOURCE, "purchase": key,
                       "url": item["url"], "kind": "info"}
            if item.get("warranty_months"):
                yield {"type": "coverage", "source": SOURCE, "purchase": key,
                       "kind": "store", "term": f"{item['warranty_months']}m",
                       "from": iso(order.get("delivered")), "issuer": "Example Shop"}
            for picture in item.get("images", []):
                yield {"type": "image", "source": SOURCE, "purchase": key,
                       "file": str((base / picture).resolve())}
        if order.get("invoice"):
            yield {"type": "document", "source": SOURCE, "purchases": keys,
                   "file": str((base / order["invoice"]).resolve()), "kind": "invoice",
                   "issued": iso(order.get("placed"))}


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--dir", default=Path(__file__).parent, type=Path)
    base = p.parse_args().dir.expanduser()
    for line in lines(base):
        print(json.dumps({k: v for k, v in line.items() if v is not None}, ensure_ascii=False))


if __name__ == "__main__":
    sys.exit(main())
