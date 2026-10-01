#!/usr/bin/env python3
"""Robotistan purchases, rebuilt from e-mails and their invoices -> ev purchase NDJSON (§6).

There is no order-history export; an agent copied the shop's mails and their attachments under
~/.ev/purchases/robotistan/ (outside every repository; they carry addresses):

  raw/mail/<rowid>[.partial].emlx      order confirmations ("Sipariş numaranız: TS..."),
                                       shipping notices, invoice mails
  raw/invoices/<rowid>-<name>.xml      a UBL-TR e-Archive invoice: the best source, one
                                       InvoiceLine per product (code, description, qty, net
                                       amount, VAT), the order number as a BARCODE reference
  raw/invoices/<rowid>-<name>.pdf      the invoice as PDF; when no XML came with it, its lines
                                       are read from the PDF's text (see pdf_text)

The lines of an order come from its invoice; an order whose mails only state a total is left
out, since nothing says what was in it. Fee lines (card commission, shipping) are not products
and are skipped. `paid` is the line's net amount plus its VAT. The same invoice mailed twice is
read once. An order number starts with the order's day and month (checked against the dated
confirmation mails), which dates an order whose confirmation mail is missing; the year is the
invoice's. Keys are `<order no>:<product code>[:n]`. Usage:

  tools/purchases/robotistan.py [--dir ~/.ev/purchases/robotistan] | ev buy import --stdin
"""
import argparse
import datetime as dt
import email
import json
import os
import re
import sys
import xml.etree.ElementTree as ET
import zlib
from email import policy
from email.utils import parsedate_to_datetime
from html import unescape
from pathlib import Path

SOURCE = "robotistan"
NS = {
    "cbc": "urn:oasis:names:specification:ubl:schema:xsd:CommonBasicComponents-2",
    "cac": "urn:oasis:names:specification:ubl:schema:xsd:CommonAggregateComponents-2",
}
# Invoice lines that are charges, not things.
FEE = re.compile(r"komisyon|kargo|teslimat|hizmet bedeli", re.I)
CONSUMABLE = ["lehim teli", "flux", "pasta", "temizleyici", "alkol", "yapıştırıcı", "pil "]


def tr_lower(s: str) -> str:
    return (s or "").replace("İ", "i").replace("I", "ı").lower()


def bucket(name: str) -> str:
    # Electronic components, tools and kits are kept; only what is used up is consumable.
    t = tr_lower(name) + " "
    return "consumable" if any(w in t for w in CONSUMABLE) else "durable"


def tr_money(s: str) -> float:
    return float(s.replace(".", "").replace(",", "."))


# --- mails -----------------------------------------------------------------------------------

def mail(path: Path):
    b = path.read_bytes()
    n, rest = b.split(b"\n", 1)
    msg = email.message_from_bytes(rest[: int(n)], policy=policy.default)
    part = msg.get_body(("html", "plain"))
    try:
        h = part.get_content() if part else ""
    except Exception:
        h = ""
    h = re.sub(r"(?is)<(script|style).*?</\1>", "", h)
    return msg, re.sub(r"\s+", " ", unescape(re.sub(r"<[^>]+>", " ", h)))


# --- invoices --------------------------------------------------------------------------------

def from_xml(path: Path) -> dict:
    r = ET.parse(path).getroot()
    order = None
    for ref in r.findall("cac:AdditionalDocumentReference", NS):
        if (ref.findtext("cbc:DocumentType", "", NS) or "").strip() == "BARCODE1":
            order = ref.findtext("cbc:ID", "", NS).strip()
    lines = []
    for il in r.findall("cac:InvoiceLine", NS):
        item = il.find("cac:Item", NS)
        net = float(il.findtext("cbc:LineExtensionAmount", "0", NS))
        tax = sum(float(t.findtext("cbc:TaxAmount", "0", NS)) for t in il.findall("cac:TaxTotal", NS))
        code = item.findtext("cac:SellersItemIdentification/cbc:ID", "", NS) or item.findtext("cbc:Name", "", NS)
        lines.append({
            "code": code.strip(),
            "name": (item.findtext("cbc:Description", "", NS) or item.findtext("cbc:Name", "", NS)).strip(),
            "qty": int(float(il.findtext("cbc:InvoicedQuantity", "1", NS))),
            "paid": round(net + tax, 2),
        })
    # Billed to a company when the buyer is identified by a VKN (a person has a TCKN).
    party = r.find("cac:AccountingCustomerParty/cac:Party", NS)
    billed_to = None
    if party is not None and any(
        i.get("schemeID") == "VKN" for i in party.findall("cac:PartyIdentification/cbc:ID", NS)
    ):
        billed_to = (party.findtext("cac:PartyName/cbc:Name", "", NS) or "").strip() or None
    return {
        "billed_to": billed_to,
        "number": r.findtext("cbc:ID", "", NS).strip(),
        "ettn": r.findtext("cbc:UUID", "", NS).strip() or None,
        "issued": r.findtext("cbc:IssueDate", "", NS).strip(),
        "order": order,
        "lines": lines,
    }


def pdf_text(path: Path) -> list[list[tuple[float, float, str]]]:
    """Text runs (y, x, text) per page of a simple PDF: Flate streams, Type0 fonts with a
    ToUnicode map, text placed with Tm and drawn with TJ/Tj. Enough for the invoice PDFs the
    shop's accounting software writes; not a general PDF reader."""
    d = path.read_bytes()
    objs = {int(m.group(1)): m.group(2) for m in re.finditer(rb"(\d+) 0 obj(.*?)endobj", d, re.S)}

    def stream(n: int) -> bytes:
        body = objs[n]
        m = re.search(rb"stream\r?\n(.*?)\r?\nendstream", body, re.S)
        raw = m.group(1)
        return zlib.decompress(raw) if b"/FlateDecode" in body.split(b"stream")[0] else raw

    def hexstr(h: bytes) -> str:
        return bytes.fromhex(h.decode()).decode("utf-16-be", "replace")

    def cmap(n: int) -> dict[int, str]:
        s, mp = stream(n), {}
        for blk in re.findall(rb"beginbfchar(.*?)endbfchar", s, re.S):
            for a, b in re.findall(rb"<([0-9A-Fa-f]+)>\s*<([0-9A-Fa-f]+)>", blk):
                mp[int(a, 16)] = hexstr(b)
        for blk in re.findall(rb"beginbfrange(.*?)endbfrange", s, re.S):
            for a, b, c in re.findall(
                rb"<([0-9A-Fa-f]+)>\s*<([0-9A-Fa-f]+)>\s*(<[0-9A-Fa-f]+>|\[[^\]]*\])", blk
            ):
                lo, hi = int(a, 16), int(b, 16)
                if c.startswith(b"["):
                    for i, v in enumerate(re.findall(rb"<([0-9A-Fa-f]+)>", c)):
                        mp[lo + i] = hexstr(v)
                else:
                    base = int(c[1:-1], 16)
                    for i in range(hi - lo + 1):
                        mp[lo + i] = chr(base + i)
        return mp

    def sub(body: bytes, key: bytes) -> bytes:
        m = re.search(rb"/" + key + rb"\s*(\d+) 0 R", body)
        if m:
            return objs[int(m.group(1))]
        m = re.search(rb"/" + key + rb"\s*<<(.*?)>>", body, re.S)
        return m.group(1) if m else b""

    pages = []
    for n in sorted(objs):
        body = objs[n]
        if not re.search(rb"/Type\s*/Page\b(?!s)", body):
            continue
        fonts = {}
        for name, ref in re.findall(rb"/(\w+)\s+(\d+) 0 R", sub(sub(body, rb"Resources"), rb"Font")):
            tu = re.search(rb"/ToUnicode\s+(\d+) 0 R", objs[int(ref)])
            fonts[name.decode()] = cmap(int(tu.group(1))) if tu else {}
        refs = re.search(rb"/Contents\s*\[?([\d\sR]+)\]?", body).group(1)
        content = b"".join(stream(int(r)) for r in re.findall(rb"(\d+) 0 R", refs))
        runs, font, x, y = [], {}, 0.0, 0.0
        for t in re.finditer(
            rb"/(\w+)\s+[\d.]+\s+Tf"
            rb"|(?:[-\d.]+\s+){4}([-\d.]+)\s+([-\d.]+)\s+Tm"
            rb"|\[(.*?)\]\s*TJ|<([0-9A-Fa-f]*)>\s*Tj",
            content,
            re.S,
        ):
            if t.group(1):
                font = fonts.get(t.group(1).decode(), {})
            elif t.group(2):
                x, y = float(t.group(2)), float(t.group(3))
            else:
                hexes = re.findall(rb"<([0-9A-Fa-f]*)>", t.group(4)) if t.group(4) is not None else [t.group(5)]
                s = "".join(font.get(int(h[i:i + 4], 16), "") for h in hexes for i in range(0, len(h), 4))
                runs.append((y, x, s))
        pages.append(runs)
    return pages


def from_pdf(path: Path) -> dict:
    """Invoice lines from the PDF's table: a row has a product code, "<n> Adet" and amounts in
    the header's columns; a description that wraps continues on the next runs of that column."""
    lines, number, issued, ettn, buyer, company = [], None, None, None, None, False
    for runs in pdf_text(path):
        ordered_runs = sorted(runs)
        for i, (y, x, s) in enumerate(ordered_runs):
            m = re.match(r"\s*(?:Fatura|Belge) No:\s*(\S+)", s)
            if m:
                number = m.group(1)
            if re.fullmatch(r"\s*[A-Z]{3}\d{13}\s*", s) and not number:
                number = s.strip()
            m = re.fullmatch(r"\s*(\d{2})-(\d{2})-(\d{4})(?: \d{2}:\d{2})?\s*", s)
            if m and not issued:
                issued = f"{m.group(3)}-{m.group(2)}-{m.group(1)}"
            if re.fullmatch(r"\s*[0-9A-F]{8}(-[0-9A-F]{4}){3}-[0-9A-F]{12}\s*", s) and not ettn:
                ettn = s.strip()
            # The buyer: "Sayın:" and the name beside it. A 10-digit tax number is a company's
            # (a person's is 11 digits), and then the invoice is billed to that company.
            if s.strip() == "Sayın:" and i + 1 < len(ordered_runs) and ordered_runs[i + 1][0] == y:
                buyer = ordered_runs[i + 1][2].strip()
            m = re.match(r"\s*VKN:\s*(\d+)", s)
            if m and len(m.group(1)) == 10:
                company = True
        head = {s.strip(): (y, x) for y, x, s in runs
                if s.strip() in ("Ürün Kodu", "Açıklama", "KDV", "Adet", "Tutar")}
        if len(head) < 5:
            continue
        top = head["Ürün Kodu"][0]
        bottom = min([y for y, x, s in runs if "Fatura Toplamı" in s and y > top], default=1e9)
        col = lambda x: min(head, key=lambda k: abs(head[k][1] - x))  # noqa: E731
        rows: dict[float, dict] = {}
        for y, x, s in ordered_runs:
            if not top < y < bottom:
                continue
            c = col(x)
            if abs(head[c][1] - x) > 15:
                continue
            rows.setdefault(round(y, 1), {}).setdefault(c, "")
            rows[round(y, 1)][c] += s
        cur = None
        for y in sorted(rows):
            r = rows[y]
            if "Ürün Kodu" in r and re.fullmatch(r"\s*\S+\s*", r["Ürün Kodu"]) and "Adet" in r:
                cur = {
                    "code": r["Ürün Kodu"].strip(),
                    "name": r.get("Açıklama", "").strip(),
                    "qty": int(re.match(r"\s*(\d+)", r["Adet"]).group(1)),
                    "net": tr_money(re.search(r"([\d.,]+)", r.get("Tutar", "0")).group(1)),
                    "vat": tr_money(re.search(r"([\d.,]+)", r.get("KDV", "0")).group(1)),
                }
                lines.append(cur)
            elif cur and set(r) == {"Açıklama"}:
                # "608-" + "39" is one word broken at its hyphen; "Seti -" + "50in1" is not.
                glue = "" if re.search(r"\S-$", cur["name"]) else " "
                cur["name"] = (cur["name"] + glue + r["Açıklama"].strip()).strip()
            else:
                cur = None
    for line in lines:
        line["name"] = re.sub(r"\s*/\s*Menşei:\s*\w*\s*$", "", line["name"]).strip()
        line["paid"] = round(line.pop("net") * (1 + line.pop("vat") / 100), 2)
    return {"number": number, "ettn": ettn, "issued": issued, "order": None, "lines": lines,
            "billed_to": buyer if company else None}


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--dir", default="~/.ev/purchases/robotistan")
    args = ap.parse_args()
    root = Path(os.path.expanduser(args.dir))

    # Orders seen in the mails: number -> confirmation date, ship date, mail file.
    orders: dict[str, dict] = {}
    for path in sorted((root / "raw" / "mail").glob("*.emlx")):
        msg, text = mail(path)
        try:
            at = parsedate_to_datetime(msg["date"]).date().isoformat()
        except (TypeError, ValueError):
            at = None
        subject = msg["subject"] or ""
        for no in set(re.findall(r"\bTS\d{6,}\b", subject + " " + text)):
            o = orders.setdefault(no, {"mails": []})
            o["mails"].append(path)
            if re.search(r"Sipariş(iniz)? (Onaylandı|Alındı)", subject):
                o["ordered"], o["raw"] = at, path
            elif re.search(r"kargo", subject, re.I):
                o["shipped"] = at

    # Invoices, one per document (the same one may arrive twice); the XML wins over its PDF.
    invoices: dict[str, tuple[dict, Path]] = {}
    for x in sorted((root / "raw" / "invoices").glob("*.xml")):
        inv = from_xml(x)
        pdf = x.with_suffix(".pdf")
        invoices.setdefault(inv["number"], (inv, pdf if pdf.exists() else x))
    for p in sorted((root / "raw" / "invoices").glob("*.pdf")):
        if p.with_suffix(".xml").exists():
            continue
        try:
            inv = from_pdf(p)
        except Exception:
            continue
        if not inv["lines"] or not inv["number"]:
            continue
        # A PDF without the order number in it is named after the order it was asked for by.
        if not inv["order"]:
            m = re.search(r"(TS\d{6,})", p.name)
            inv["order"] = m.group(1) if m else None
        invoices.setdefault(inv["number"], (inv, p))

    out = sys.stdout

    def emit(rec: dict) -> None:
        out.write(json.dumps({k: v for k, v in rec.items() if v is not None}, ensure_ascii=False))
        out.write("\n")

    for inv, file in sorted(invoices.values(), key=lambda v: v[0]["issued"] or ""):
        order = inv["order"] or inv["number"]
        o = orders.get(order, {})
        ordered = o.get("ordered")
        m = re.match(r"TS(\d{2})(\d{2})", order)
        if not ordered and m and inv["issued"]:
            try:
                ordered = dt.date(int(inv["issued"][:4]), int(m.group(2)), int(m.group(1))).isoformat()
            except ValueError:
                ordered = None
            if ordered and ordered > inv["issued"]:
                ordered = None
        keys, seen = [], {}
        for line in inv["lines"]:
            if FEE.search(line["name"]):
                continue
            n = seen.get(line["code"], 0)
            seen[line["code"]] = n + 1
            key = f"{order}:{line['code']}" if n == 0 else f"{order}:{line['code']}:{n + 1}"
            b = bucket(line["name"])
            if b != "consumable":
                keys.append(key)
            emit({
                "type": "purchase",
                "source": SOURCE,
                "key": key,
                "shop": "Robotistan",
                "merchant": "Robotistan",
                "order": order,
                "sku": line["code"],
                "name": line["name"],
                "ordered_at": ordered or inv["issued"],
                "qty": max(line["qty"], 1),
                "paid": f"{line['paid']:.2f}",
                "currency": "TRY",
                "billed_to": inv.get("billed_to"),
                "status": "delivered",
                "bucket": b,
                "raw": str(o.get("raw") or file),
            })
        if keys:
            emit({
                "type": "document",
                "source": SOURCE,
                "kind": "invoice",
                "file": str(file),
                "purchases": keys,
                "number": inv["number"],
                "ettn": inv["ettn"],
                "issued": inv["issued"],
                "issuer": "Robotistan",
            })


if __name__ == "__main__":
    main()
