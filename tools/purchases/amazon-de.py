#!/usr/bin/env python3
"""amazon.de order history -> ev purchase NDJSON (spec/purchases.md §6).

The account could not be logged into, so the only source is the shop's mails, parsed by an
agent under ~/.ev/purchases/amazon-de/ (outside every repository; the mails carry addresses):

  catalog.json             one object per item a mail names: order_no, title, qty, price_eur (the
                           unit price as German text, "1.234,56"), digital, first_seen, mails
                           (rowid, kind: confirmation|shipped, date), order_flags (cancelled,
                           returned_or_refunded: a later mail names the order)
  messages.json            every mail: rowid, kind, date, the orders it names
  raw/mail/<rowid>[.partial].emlx   the mails themselves
  raw/attachments/<rowid>-<name>.pdf   their attachments: seller invoices, credit notes, terms

What the mails cannot tell, and how it is handled:

- Order lines carry no ASIN, so the key is the order number plus a short hash of the title.
- Some mail layouts put address, delivery-option and gift-message lines next to the items; the
  parser kept a few. A line whose title ends in a colon or starts with "von", or that has no
  price and is not digital, is dropped as one of those.
- Cancellation and refund mails name the order, not the item: a flag is on every line of the
  order. A line in a cancelled order that no shipping mail names was never sent (cancelled,
  dropped); every other flagged line is marked `returned`. A partial refund therefore marks
  kept lines too; the status is editable in ev.
- No delivery date. `ordered_at` is the order confirmation's date, or the first shipping
  notice's when there is no confirmation (the order was placed on or before it).
- `paid` is the unit price times the quantity; shipping and vouchers are not in the item lines.

Invoices: an attachment is an invoice when the mail that carried it names an order and its
file name is not a credit note or a terms/withdrawal/privacy leaflet; it is hung on every line
of that order. Attachments whose mail names no order are left out. Usage:

  tools/purchases/amazon-de.py [--dir ~/.ev/purchases/amazon-de] | ev buy import --stdin
"""
import argparse
import hashlib
import json
import os
import re
import sys
from pathlib import Path

SOURCE = "amazon-de"

# Regexes searched in the lowercased title; each starts at a word boundary unless it is a
# compound tail (German glues words together: "Alkalibatterien", "Staubsaugerbeutel").
STRONG = [r"waschladungen", r"waschmittel", r"eau de toilette", r"\btoner\b", r"druckerpatronen"]
CONSUMABLE = [
    r"alkali", r"staubsaugerbeutel", r"aufsteckbürsten", r"entkalker", r"wasserfilter",
    r"pflegemittel", r"kriechöl", r"feuchtigkeitspflege", r"druckverschlussbeutel", r"weichspüler",
    r"shampoo", r"\bkaffee\b", r"\btee\b", r"futter", r"filmblut",
]
KEEP = [r"ladegerät", r"\bakku", r"aufbewahrungsbox", r"eneloop", r"maschine"]
CLOTHING = [
    r"handschuhe", r"mütze", r"\bponcho", r"jacke\b", r"\bhose\b", r"shirt\b", r"socken",
    r"schuhe\b", r"\bpullover",
]
DIGITAL = [r"kindle edition", r"\bamazon prime\b", r"\bmp3\b"]
NOT_DOCUMENT = [r"credit-note", r"agb", r"datenschutz", r"widerruf", r"bitte-lesen", r"entsorgung"]


def bucket(title: str, digital: bool) -> str:
    t = title.lower()
    if digital or any(re.search(p, t) for p in DIGITAL):
        return "digital"
    if any(re.search(p, t) for p in CLOTHING):
        return "clothing"
    if any(re.search(p, t) for p in STRONG):
        return "consumable"
    if any(re.search(p, t) for p in CONSUMABLE) and not any(re.search(p, t) for p in KEEP):
        return "consumable"
    return "durable"


def euros(text: str | None) -> float | None:
    if not text:
        return None
    try:
        return float(text.replace(".", "").replace(",", "."))
    except ValueError:
        return None


def is_noise(line: dict) -> bool:
    t = line["title"].strip()
    if t.endswith(":") or re.match(r"(?i)von\b", t):
        return True
    return line.get("price_eur") is None and not line.get("digital")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--dir", default="~/.ev/purchases/amazon-de")
    args = ap.parse_args()
    root = Path(os.path.expanduser(args.dir))
    catalog = json.loads((root / "catalog.json").read_text())
    messages = {m["rowid"]: m for m in json.loads((root / "messages.json").read_text())}
    mail_dir = root / "raw" / "mail"

    keys_by_order: dict[str, list[str]] = {}
    seen: dict[tuple[str, str], int] = {}
    out = sys.stdout
    for line in catalog:
        if is_noise(line):
            continue
        order, title = line["order_no"], line["title"].strip()
        kinds = {m["kind"] for m in line.get("mails", [])}
        flags = set(line.get("order_flags") or [])
        if "cancelled" in flags and "shipped" not in kinds:
            continue
        status = "returned" if flags else "delivered"
        digest = hashlib.sha1(title.encode()).hexdigest()[:10]
        n = seen.get((order, digest), 0)
        seen[(order, digest)] = n + 1
        key = f"{order}:{digest}" if n == 0 else f"{order}:{digest}:{n + 1}"
        keys_by_order.setdefault(order, []).append(key)

        mails = sorted(line.get("mails", []), key=lambda m: (m.get("date") or "", m["rowid"]))
        confirmations = [m for m in mails if m["kind"] == "confirmation"]
        first = (confirmations or mails or [{}])[0]
        raw = None
        for suffix in (".emlx", ".partial.emlx"):
            p = mail_dir / f"{first.get('rowid')}{suffix}"
            if p.exists():
                raw = str(p)
                break
        qty = line.get("qty") or 1
        unit = euros(line.get("price_eur"))
        rec = {
            "type": "purchase",
            "source": SOURCE,
            "key": key,
            "shop": "Amazon.de",
            "order": order,
            "order_url": f"https://www.amazon.de/gp/your-account/order-details?orderID={order}",
            "name": title,
            "ordered_at": first.get("date") or line.get("first_seen"),
            "qty": qty,
            "paid": f"{unit * qty:.2f}" if unit is not None else None,
            "currency": "EUR",
            "status": status,
            "bucket": bucket(title, line.get("digital", False)),
            "raw": raw,
        }
        out.write(json.dumps({k: v for k, v in rec.items() if v is not None}, ensure_ascii=False))
        out.write("\n")

    for pdf in sorted((root / "raw" / "attachments").glob("*")):
        if not pdf.name.lower().rstrip(" .").endswith(".pdf"):
            continue
        if any(re.search(p, pdf.name.lower()) for p in NOT_DOCUMENT):
            continue
        rowid = pdf.name.split("-", 1)[0]
        mail = messages.get(int(rowid)) if rowid.isdigit() else None
        if not mail:
            continue
        keys = [k for o in mail.get("orders", []) for k in keys_by_order.get(o, [])]
        if not keys:
            continue
        doc = {
            "type": "document",
            "source": SOURCE,
            "kind": "invoice",
            "file": str(pdf),
            "purchases": keys,
        }
        out.write(json.dumps({k: v for k, v in doc.items() if v is not None}, ensure_ascii=False))
        out.write("\n")


if __name__ == "__main__":
    main()
