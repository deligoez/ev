#!/usr/bin/env python3
"""sahibinden.com purchases (S-Param escrow), rebuilt from e-mails -> ev purchase NDJSON (§6).

The site's own purchase list came back empty, so the record is the buyer-side mail of the
escrow ("S-Param Güvende") flow. An agent copied it under ~/.ev/purchases/sahibinden/ (outside
every repository; the mails carry addresses):

  raw/mail/<rowid>[.partial].emlx   Apple Mail's emlx: a byte count, then the message

Only mails addressed to the buyer are read, so listings the person sold or published never
appear. Per listing ("#<listing id>" under the title, the price under it):

  "Ödemeniz alındı ..."            the payment: ordered_at, and the price
  "Ürünü teslim aldınız ..."        delivered_at
  "Alım işleminiz tamamlandı"       the sale closed
  "... iade ..." / "... iptal ..."  returned / cancelled

`paid` is the item price. The escrow service fee is charged on top and invoiced separately by
the platform (e-Archive mails without the PDF and without the listing id), so it is not on the
line, and no `document` lines are written. Keys are the listing id. Usage:

  tools/purchases/sahibinden.py [--dir ~/.ev/purchases/sahibinden] | ev buy import --stdin
"""
import argparse
import email
import json
import os
import re
import sys
from email import policy
from email.utils import parsedate_to_datetime
from html import unescape
from pathlib import Path

SOURCE = "sahibinden"

CONSUMABLE = ["mama", "deterjan", "kozmetik", "parfüm"]
CLOTHING = ["mont", "ayakkabı", "elbise", "gömlek", "pantolon", "ceket", "kaban"]
DIGITAL = ["lisans", "dijital kod", "abonelik"]


def tr_lower(s: str) -> str:
    return (s or "").replace("İ", "i").replace("I", "ı").lower()


def bucket(title: str) -> str:
    t = tr_lower(title)
    if any(w in t for w in DIGITAL):
        return "digital"
    if any(w in t for w in CLOTHING):
        return "clothing"
    if any(w in t for w in CONSUMABLE):
        return "consumable"
    return "durable"


def read(path: Path):
    b = path.read_bytes()
    n, rest = b.split(b"\n", 1)
    msg = email.message_from_bytes(rest[: int(n)], policy=policy.default)
    part = msg.get_body(("html", "plain"))
    try:
        h = part.get_content() if part else ""
    except Exception:
        h = ""
    h = re.sub(r"(?is)<(script|style).*?</\1>", "", h)
    t = unescape(re.sub(r"<[^>]+>", "\n", h))
    return msg, [x.strip() for x in t.split("\n") if x.strip()]


def money(s: str) -> float | None:
    m = re.search(r"([\d.,]+)\s*TL", s or "")
    if not m:
        return None
    v = m.group(1)
    if "," in v:
        v = v.replace(".", "").replace(",", ".")
    elif re.fullmatch(r"\d{1,3}(\.\d{3})+", v):
        v = v.replace(".", "")
    return float(v)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--dir", default="~/.ev/purchases/sahibinden")
    args = ap.parse_args()
    root = Path(os.path.expanduser(args.dir))

    mails = []
    for path in (root / "raw" / "mail").glob("*.emlx"):
        msg, lines = read(path)
        try:
            at = parsedate_to_datetime(msg["date"])
        except (TypeError, ValueError):
            continue
        mails.append((at, path, tr_lower(msg["subject"] or ""), lines))
    mails.sort(key=lambda x: x[0])

    sales: dict[str, dict] = {}
    for at, path, subject, lines in mails:
        for i, line in enumerate(lines):
            m = re.fullmatch(r"#(\d{6,})", line)
            if not m:
                continue
            s = sales.setdefault(m.group(1), {"id": m.group(1)})
            s.setdefault("title", lines[i - 1] if i else m.group(1))
            price = money(lines[i + 1]) if i + 1 < len(lines) else None
            if "ürün tutarı" in [tr_lower(x) for x in lines]:
                j = [tr_lower(x) for x in lines].index("ürün tutarı")
                price = money(lines[j + 1]) or price
            if price is not None and ("ödemeniz alındı" in subject or "price" not in s):
                s["price"] = price
            if "ödemeniz alındı" in subject:
                s["ordered"] = at.date().isoformat()
                s["raw"] = path
            elif "teslim aldınız" in subject:
                s["delivered"] = at.date().isoformat()
            elif "iade" in subject:
                s["returned"] = True
            elif "iptal" in subject:
                s["cancelled"] = True
            s.setdefault("first", at.date().isoformat())
            s.setdefault("raw", path)

    out = sys.stdout
    for s in sorted(sales.values(), key=lambda s: s.get("ordered") or s["first"]):
        status = "cancelled" if s.get("cancelled") else "returned" if s.get("returned") else "delivered"
        rec = {
            "type": "purchase",
            "source": SOURCE,
            "key": s["id"],
            "shop": "sahibinden.com",
            "sku": s["id"],
            "name": s["title"],
            "ordered_at": s.get("ordered") or s["first"],
            "delivered_at": s.get("delivered"),
            "qty": 1,
            "paid": f"{s['price']:.2f}" if s.get("price") is not None else None,
            "currency": "TRY",
            "status": status,
            "bucket": bucket(s["title"]),
            "raw": str(s["raw"]),
        }
        out.write(json.dumps({k: v for k, v in rec.items() if v is not None}, ensure_ascii=False))
        out.write("\n")


if __name__ == "__main__":
    main()
