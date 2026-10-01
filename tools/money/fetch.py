#!/usr/bin/env python3
"""What `ev money needs` asks for -> index and rate NDJSON for `ev money import` (spec/purchases.md §3.9).

  ev money needs | tools/money/fetch.py | ev money import --stdin

The index series is the inventory's `price_index` setting, `<source>:<country>`:

  eurostat:<geo>    monthly HICP, 2025=100 (prc_hicp_minr, all items); covers the EU, the UK,
                    the US, Türkiye and the other countries Eurostat publishes
  worldbank:<iso2>  annual CPI, 2010=100 (FP.CPI.TOTL); for a country Eurostat lacks

The two are never mixed: each is its own series, with its own base. Rates are home currency per
unit of the bought one: the Central Bank of Türkiye's ForexSelling when home is TRY, the ECB's
reference rates (crossed through the euro) otherwise. A day without a rate (a weekend, a
holiday) takes the closest earlier one, and the line carries that day. No key is needed for any
of them. Standard library only.
"""
import csv
import io
import json
import sys
import urllib.error
import urllib.request
import xml.etree.ElementTree as ET
from datetime import date, timedelta

LOOKBACK_DAYS = 7
UA = {"User-Agent": "ev-money/1 (+https://github.com/deligoez/ev)"}


def get(url):
    req = urllib.request.Request(url, headers=UA)
    with urllib.request.urlopen(req, timeout=30) as r:
        return r.read()


def eurostat(geo, since):
    url = (
        "https://ec.europa.eu/eurostat/api/dissemination/statistics/1.0/data/prc_hicp_minr"
        f"?geo={geo}&unit=I25&coicop18=TOTAL&sinceTimePeriod={since}"
    )
    d = json.loads(get(url))
    periods = {i: p for p, i in d["dimension"]["time"]["category"]["index"].items()}
    for i, v in sorted(d["value"].items(), key=lambda kv: int(kv[0])):
        yield periods[int(i)], v


def worldbank(country, since):
    url = (
        f"https://api.worldbank.org/v2/country/{country}/indicator/FP.CPI.TOTL"
        f"?format=json&per_page=200&date={since[:4]}:{date.today().year}"
    )
    rows = json.loads(get(url))[1] or []
    for r in sorted(rows, key=lambda r: r["date"]):
        if r["value"] is not None:
            yield r["date"], r["value"]


def index_lines(series, since):
    source, _, place = series.partition(":")
    fetch = {"eurostat": eurostat, "worldbank": worldbank}.get(source)
    if fetch is None:
        sys.exit(f"unknown index series {series!r}: use eurostat:<geo> or worldbank:<iso2>")
    for period, value in fetch(place, since):
        yield {"type": "index", "series": series, "period": period, "value": value, "source": source}


def tcmb(currency, day):
    """TRY per unit of `currency` on `day` or the closest earlier day the bank published."""
    for back in range(LOOKBACK_DAYS + 1):
        d = day - timedelta(days=back)
        url = f"https://www.tcmb.gov.tr/kurlar/{d:%Y%m}/{d:%d%m%Y}.xml"
        try:
            root = ET.fromstring(get(url))
        except urllib.error.HTTPError:
            continue
        for c in root.iter("Currency"):
            if c.get("CurrencyCode") == currency:
                rate = c.findtext("ForexSelling") or c.findtext("BanknoteSelling")
                unit = int(c.findtext("Unit") or 1)
                if rate:
                    return d, float(rate) / unit
        return None
    return None


def ecb_series(currency, day):
    """Units of `currency` per euro, by day, for the week up to `day`."""
    if currency == "EUR":
        return None
    start = day - timedelta(days=LOOKBACK_DAYS)
    url = (
        f"https://data-api.ecb.europa.eu/service/data/EXR/D.{currency}.EUR.SP00.A"
        f"?startPeriod={start}&endPeriod={day}&format=csvdata"
    )
    try:
        rows = csv.DictReader(io.StringIO(get(url).decode()))
        return {r["TIME_PERIOD"]: float(r["OBS_VALUE"]) for r in rows if r["OBS_VALUE"]}
    except urllib.error.HTTPError:
        return {}


def ecb(currency, home, day):
    """Home per unit of `currency` on `day` or the closest earlier day both have a rate."""
    per_eur_cur = ecb_series(currency, day)
    per_eur_home = ecb_series(home, day)
    for back in range(LOOKBACK_DAYS + 1):
        d = str(day - timedelta(days=back))
        cur = 1.0 if per_eur_cur is None else per_eur_cur.get(d)
        hom = 1.0 if per_eur_home is None else per_eur_home.get(d)
        if cur and hom:
            return date.fromisoformat(d), hom / cur
    return None


def rate_lines(home, wanted):
    for w in wanted:
        currency, day = w["currency"].upper(), date.fromisoformat(w["day"][:10])
        found = tcmb(currency, day) if home == "TRY" else ecb(currency, home, day)
        if found is None:
            print(f"no {currency}->{home} rate near {day}", file=sys.stderr)
            continue
        on, rate = found
        source = "tcmb" if home == "TRY" else "ecb"
        yield {"type": "rate", "currency": currency, "home": home, "day": str(on), "rate": rate, "source": source}


def main():
    needs = json.load(sys.stdin)
    needs = needs.get("money_needs", needs)
    home = needs["home_currency"].upper()
    lines = []
    if needs.get("from_month"):
        lines += index_lines(needs["index"], needs["from_month"])
    lines += rate_lines(home, needs.get("rates", []))
    for line in lines:
        print(json.dumps(line, ensure_ascii=False))


if __name__ == "__main__":
    main()
