#!/usr/bin/env python3
"""Vivense order history -> ev purchase NDJSON (spec/purchases.md §6).

Reads the raw export an agent saved under ~/.ev/purchases/vivense/ (outside every repository;
it carries addresses and phone numbers):

  catalog.json              one object per order line: order_id, order_date, line_index, title,
                            sku (the shop's product code), quantity, line_total (page unit price x
                            qty), invoice_paid_incl_vat, invoice_no, invoice_date, invoice_file,
                            status (`teslim-edildi`, ...), product_url, order_url
  raw/orders/<order>.html   the order's block on the orders page (the raw record of a line)
  raw/invoices/<order>-L<n>.html   the e-Archive invoices: Vivense issues one per order line
  raw/docs/<sku>-<hash>.pdf        assembly instructions, when the product had them

The amount paid is the line's invoice total: the order discount, the delivery fee and the
instalment interest are spread over the per-line invoices, so their sum is what the order cost
(to the lira). Without an invoice the page price is used. The site shows no delivery date. All
products are furniture and home textiles, so every line is durable. Writes `purchase` lines,
`document` lines for the invoices (HTML: the e-Archive render the site serves) and `manual`
documents for the assembly PDFs. Usage:

  tools/purchases/vivense.py [--dir ~/.ev/purchases/vivense] | ev buy import --stdin
"""
import argparse
import json
import os
import sys
from pathlib import Path

SOURCE = "vivense"

STATUS = {"teslim-edildi": "delivered", "iade-edildi": "returned", "iptal-edildi": "cancelled"}


def iso(d: str | None) -> str | None:
    """'19.11.2022' -> '2022-11-19'."""
    if not d:
        return None
    parts = d.split(".")
    return f"{parts[2]}-{parts[1]}-{parts[0]}" if len(parts) == 3 else None


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--dir", default="~/.ev/purchases/vivense")
    args = ap.parse_args()
    root = Path(os.path.expanduser(args.dir))
    catalog = json.loads((root / "catalog.json").read_text())

    out = sys.stdout
    docs: list[dict] = []
    seen: dict[tuple[str, str], int] = {}
    for line in catalog:
        order, sku = str(line["order_id"]), line.get("sku") or f"L{line['line_index']}"
        # The same product twice in one order is two lines: the n-th of them gets `:n`.
        n = seen.get((order, sku), 0)
        seen[(order, sku)] = n + 1
        key = f"{order}:{sku}" if n == 0 else f"{order}:{sku}:{n + 1}"
        raw = root / "raw" / "orders" / f"{order}.html"
        paid = line.get("invoice_paid_incl_vat")
        if paid is None:
            paid = line.get("line_total")
        rec = {
            "type": "purchase",
            "source": SOURCE,
            "key": key,
            "shop": "Vivense",
            "order": order,
            "order_url": line.get("order_url"),
            "product_url": line.get("product_url"),
            "sku": line.get("sku"),
            "name": line["title"],
            "ordered_at": line.get("order_date"),
            "qty": line.get("quantity") or 1,
            "paid": f"{paid:.2f}" if paid is not None else None,
            "currency": line.get("currency") or "TRY",
            "status": STATUS.get(line.get("status") or "", "delivered"),
            "bucket": "durable",
            "raw": str(raw) if raw.exists() else None,
        }
        out.write(json.dumps({k: v for k, v in rec.items() if v is not None}, ensure_ascii=False))
        out.write("\n")

        invoice = root / line["invoice_file"] if line.get("invoice_file") else None
        if invoice and invoice.exists():
            docs.append({
                "type": "document",
                "source": SOURCE,
                "kind": "invoice",
                "file": str(invoice),
                "purchases": [key],
                "number": line.get("invoice_no"),
                "issued": iso(line.get("invoice_date")),
                "issuer": "Vivense",
            })
        for pdf in sorted((root / "raw" / "docs").glob(f"{line.get('sku')}-*.pdf")):
            docs.append({
                "type": "document",
                "source": SOURCE,
                "kind": "manual",
                "file": str(pdf),
                "purchases": [key],
                "note": "assembly instructions",
            })

    for doc in docs:
        out.write(json.dumps({k: v for k, v in doc.items() if v is not None}, ensure_ascii=False))
        out.write("\n")


if __name__ == "__main__":
    main()
