#!/usr/bin/env python3
"""idefix order history -> ev purchase NDJSON (spec/purchases.md §6).

Reads the raw export an agent saved under ~/.ev/purchases/idefix/ (outside every repository):

  raw/api/old-order-<order>.json   one file per order, as the shop's old-orders endpoint
                                   returned it: a list with one order {orderTotal,
                                   orderTotalDiscount, shipmentTotal, items[{productName,
                                   quantity, orderItemStatus, subTotal, unitPrice, ...}]}

The endpoint gives no order date, no product id or URL and no invoice, so lines carry only the
order number, name, quantity, amount and status. Amounts are Turkish-formatted strings
('1.234,56 TL'); `subTotal` is the line total after the line's discount (shipping excluded).
The product has no id, so a line's key is its position in the order. "Faturası Kesildi"
(invoiced) on these old orders is read as delivered. Books and electronics are durable. Usage:

  tools/purchases/idefix.py [--dir ~/.ev/purchases/idefix] | ev buy import --stdin
"""
import argparse
import json
import os
import re
import sys
from pathlib import Path

SOURCE = "idefix"


def tr_lower(s: str) -> str:
    return (s or "").replace("İ", "i").replace("I", "ı").lower()


def amount(text) -> str | None:
    """'1.234,56 TL' -> '1234.56'."""
    m = re.search(r"[\d.]+(?:,\d+)?", str(text or ""))
    if not m:
        return None
    return f"{float(m.group(0).replace('.', '').replace(',', '.')):.2f}"


def status(text: str) -> str:
    t = tr_lower(text)
    if "iptal" in t:
        return "cancelled"
    if "iade" in t:
        return "returned"
    return "delivered"


def bucket(name: str) -> str:
    t = tr_lower(name)
    if re.search(r"e-kitap|dijital|abonelik", t):
        return "digital"
    return "durable"


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--dir", default="~/.ev/purchases/idefix")
    args = ap.parse_args()
    root = Path(os.path.expanduser(args.dir))

    out = sys.stdout
    for raw in sorted((root / "raw" / "api").glob("old-order-*.json")):
        order = raw.stem.removeprefix("old-order-")
        data = json.loads(raw.read_text())
        records = data if isinstance(data, list) else [data]
        n = 0
        for rec_order in records:
            for item in rec_order.get("items") or []:
                n += 1
                rec = {
                    "type": "purchase",
                    "source": SOURCE,
                    "key": f"{order}:{n}",
                    "shop": "idefix",
                    "order": order,
                    "name": item["productName"],
                    "qty": int(item.get("quantity") or 1),
                    "paid": amount(item.get("subTotal")),
                    "currency": "TRY",
                    "status": status(item.get("orderItemStatus") or ""),
                    "bucket": bucket(item["productName"]),
                    "raw": str(raw),
                }
                out.write(json.dumps({k: v for k, v in rec.items() if v is not None}, ensure_ascii=False))
                out.write("\n")


if __name__ == "__main__":
    main()
