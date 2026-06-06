#!/usr/bin/env python3

import argparse
import json
import re
import sys
from html.parser import HTMLParser
from urllib.error import HTTPError, URLError
from urllib.parse import urlparse
from urllib.request import Request, urlopen

REQUEST_TIMEOUT_SECONDS = 20
MAX_RESPONSE_BYTES = 5 * 1024 * 1024
USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0 Safari/537.36"
TITLE_PATTERN = re.compile(r"<title[^>]*>(.*?)</title>", re.IGNORECASE | re.DOTALL)
META_PATTERN = re.compile(
    r'<meta\s+[^>]*(?:name|property)=["\']([^"\']+)["\'][^>]*content=["\']([^"\']*)["\'][^>]*>|'
    r'<meta\s+[^>]*content=["\']([^"\']*)["\'][^>]*(?:name|property)=["\']([^"\']+)["\'][^>]*>',
    re.IGNORECASE,
)
PRICE_PATTERN = re.compile(r'"(?:price|priceRange|salePrice|discountPrice)"\s*:\s*"?([^",}]+)"?', re.IGNORECASE)
IMAGE_PATTERN = re.compile(r'https?:\\?/\\?/[^"\']+?\.(?:jpg|jpeg|png|webp)', re.IGNORECASE)
WHITESPACE_PATTERN = re.compile(r"\s+")


class TextCleaner(HTMLParser):
    def __init__(self):
        super().__init__()
        self.parts = []

    def handle_data(self, data):
        text = data.strip()
        if text:
            self.parts.append(text)

    def value(self):
        return WHITESPACE_PATTERN.sub(" ", " ".join(self.parts)).strip()


def clean_html_text(value):
    parser = TextCleaner()
    parser.feed(value)
    return parser.value()


def normalize_url(url):
    parsed = urlparse(url)
    if parsed.scheme in {"http", "https"} and parsed.netloc:
        return url
    return f"https://{url}"


def fetch_html(url):
    request = Request(
        url,
        headers={
            "User-Agent": USER_AGENT,
            "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            "Accept-Language": "zh-CN,zh;q=0.9,en;q=0.8",
        },
    )
    with urlopen(request, timeout=REQUEST_TIMEOUT_SECONDS) as response:
        return response.read(MAX_RESPONSE_BYTES).decode("utf-8", errors="replace")


def extract_meta(html):
    meta = {}
    for match in META_PATTERN.finditer(html):
        key = match.group(1) or match.group(4)
        value = match.group(2) or match.group(3)
        if key and value:
            meta[key.lower()] = clean_html_text(value)
    return meta


def extract_title(html, meta):
    for key in ("og:title", "title", "twitter:title"):
        if meta.get(key):
            return meta[key]
    match = TITLE_PATTERN.search(html)
    return clean_html_text(match.group(1)) if match else ""


def extract_price(html):
    prices = []
    for match in PRICE_PATTERN.finditer(html):
        price = match.group(1).strip()
        if price and price not in prices:
            prices.append(price)
    return prices


def extract_images(html, meta):
    images = []
    for key in ("og:image", "twitter:image"):
        if meta.get(key):
            images.append(meta[key])
    for match in IMAGE_PATTERN.finditer(html):
        image = match.group(0).replace("\\/", "/")
        if image.startswith("http:\"):
            image = image.replace("http:\", "http:", 1)
        if image.startswith("https:\"):
            image = image.replace("https:\", "https:", 1)
        if image not in images:
            images.append(image)
    return images[:20]


def crawl_product(url):
    normalized_url = normalize_url(url)
    html = fetch_html(normalized_url)
    meta = extract_meta(html)
    return {
        "url": normalized_url,
        "title": extract_title(html, meta),
        "description": meta.get("description") or meta.get("og:description") or "",
        "keywords": meta.get("keywords", ""),
        "prices": extract_price(html),
        "images": extract_images(html, meta),
    }


def main():
    parser = argparse.ArgumentParser(description="Crawl basic metadata from a 1688 product page.")
    parser.add_argument("url", help="1688 product URL, for example https://detail.1688.com/offer/xxxx.html")
    parser.add_argument("-o", "--output", help="Write JSON result to a file")
    args = parser.parse_args()

    try:
        result = crawl_product(args.url)
    except (HTTPError, URLError, TimeoutError) as error:
        print(f"Failed to crawl product: {error}", file=sys.stderr)
        return 1

    content = json.dumps(result, ensure_ascii=False, indent=2)
    if args.output:
        with open(args.output, "w", encoding="utf-8") as file:
            file.write(content)
            file.write("\n")
    else:
        print(content)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
