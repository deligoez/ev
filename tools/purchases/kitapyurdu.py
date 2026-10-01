#!/usr/bin/env python3
"""Kitapyurdu order history -> ev purchase NDJSON (spec/purchases.md §6).

Reads the raw export an agent saved under ~/.ev/purchases/kitapyurdu/ (outside every
repository; the order pages carry addresses and phone numbers):

  catalog.json                 one object per bought line: order_id, order_no, order_date
                               (DD.MM.YYYY), title, status, qty, unit_price, line_total,
                               product_url, product_id, page_file
  raw/orders/<order_id>.html   the order page a line points at; newer ones say on which day
                               the order was delivered
  raw/products/<id>.html       the public product page; its JSON-LD names the publisher

Line totals on the order page include VAT; shipping is charged per order and is not spread over
the lines. The shop issues no invoice files into the export, so no `document` lines are written.
Usage:

  tools/purchases/kitapyurdu.py [--dir ~/.ev/purchases/kitapyurdu] | ev buy import --stdin
"""
import argparse
import html
import json
import os
import re
import sys
from pathlib import Path

SOURCE = "kitapyurdu"
ORDER_URL = "https://www.kitapyurdu.com/index.php?route=account/order/info&order_id={}"


def iso(dmy: str | None) -> str | None:
    m = re.match(r"(\d{2})\.(\d{2})\.(\d{4})", dmy or "")
    return f"{m.group(3)}-{m.group(2)}-{m.group(1)}" if m else None


def page_text(path: Path) -> str:
    if not path.exists():
        return ""
    t = path.read_text(errors="ignore")
    t = re.sub(r"(?is)<(script|style).*?</\1>", " ", t)
    return re.sub(r"\s+", " ", html.unescape(re.sub(r"<[^>]+>", " ", t)))


def publisher(path: Path) -> str | None:
    """The product's brand (the publisher) from the page's JSON-LD."""
    if not path.exists():
        return None
    m = re.search(r'"brand":\{"@type":"Brand","name":"([^"]+)"', path.read_text(errors="ignore"))
    return html.unescape(m.group(1)).strip() if m else None


def status(line_status: str, order_text: str) -> str:
    s = (line_status or "").upper()
    if "İPTAL" in s or "IPTAL" in s:
        return "cancelled"
    if "İADE" in s or "IADE" in s:
        return "returned"
    # The order's own state is the first "Durumu:" on the page.
    m = re.search(r"Durumu: ([A-ZÇĞİÖŞÜ ]+?) ", order_text)
    o = m.group(1) if m else ""
    if "İPTAL" in o:
        return "cancelled"
    if "İADE" in o:
        return "returned"
    return "delivered"


def bucket(url: str, title: str) -> str:
    u = (url or "").lower()
    if "e-kitap" in u or "/ekitap" in u or "e-kitap" in (title or "").lower():
        return "digital"
    # Books are kept; the shop's few non-book lines (stationery, games) are kept too.
    return "durable"


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--dir", default="~/.ev/purchases/kitapyurdu")
    args = ap.parse_args()
    root = Path(os.path.expanduser(args.dir))
    catalog = json.loads((root / "catalog.json").read_text())

    out = sys.stdout
    seen: dict[tuple[str, str], int] = {}
    pages: dict[str, str] = {}
    for line in catalog:
        order, pid = line["order_id"], line.get("product_id") or ""
        sku = pid or re.sub(r"\W+", "-", line["title"]).strip("-").lower()
        n = seen.get((order, sku), 0)
        seen[(order, sku)] = n + 1
        key = f"{order}:{sku}" if n == 0 else f"{order}:{sku}:{n + 1}"
        raw = root / "raw" / "orders" / f"{order}.html"
        if order not in pages:
            pages[order] = page_text(raw)
        text = pages[order]
        m = re.search(r"(\d{2}\.\d{2}\.\d{4}) tarihinde teslim", text)
        paid = line.get("line_total")
        rec = {
            "type": "purchase",
            "source": SOURCE,
            "key": key,
            "shop": "Kitapyurdu",
            "order": line.get("order_no"),
            "order_url": ORDER_URL.format(order),
            "product_url": line.get("product_url"),
            "sku": pid or None,
            "name": line["title"],
            "brand": publisher(root / line["page_file"]) if line.get("page_file") else None,
            "category": "Kitap" if "/kitap/" in (line.get("product_url") or "") else None,
            "ordered_at": iso(line.get("order_date")),
            "delivered_at": iso(m.group(1)) if m else None,
            "qty": line.get("qty") or 1,
            "paid": f"{paid:.2f}" if paid is not None else None,
            "currency": "TRY",
            "status": status(line.get("status", ""), text),
            "bucket": bucket(line.get("product_url", ""), line["title"]),
            "raw": str(raw) if raw.exists() else None,
        }
        out.write(json.dumps({k: v for k, v in rec.items() if v is not None}, ensure_ascii=False))
        out.write("\n")


if __name__ == "__main__":
    main()
