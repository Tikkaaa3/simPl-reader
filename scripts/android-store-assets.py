"""Render repository artwork and store copy reproducibly; requires Pillow."""
from pathlib import Path
import re
import xml.etree.ElementTree as ET
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
STORE = ROOT / "android/store"


def icon():
    # Use the existing vector paths, with a square background for Play's mask.
    image = Image.new("RGBA", (1024, 1024), "#0c0c0c")
    draw = ImageDraw.Draw(image)
    for path in ET.parse(ROOT / "installer/simPl.svg").iter("{http://www.w3.org/2000/svg}path"):
        tokens = iter(re.findall(r"[MQV]|-?\d+(?:\.\d+)?", path.attrib["d"]))
        points = []
        current = (0.0, 0.0)
        def point():
            return float(next(tokens)), float(next(tokens))
        def stroke():
            if len(points) > 1:
                scaled = [(round(x * 4), round(y * 4)) for x, y in points]
                draw.line(scaled, fill="#ededed", width=40, joint="curve")
                for x, y in (scaled[0], scaled[-1]):
                    draw.ellipse((x - 20, y - 20, x + 20, y + 20), fill="#ededed")
        for command in tokens:
            if command == "M":
                stroke(); current = point(); points = [current]
            elif command == "V":
                current = current[0], float(next(tokens)); points.append(current)
            elif command == "Q":
                control, end = point(), point()
                for step in range(1, 33):
                    t = step / 32
                    points.append(tuple((1-t)**2 * current[i] + 2 * (1-t)*t * control[i] + t*t * end[i] for i in range(2)))
                current = end
            else:
                raise ValueError(f"Unsupported icon command: {command}")
        stroke()
    image.resize((512, 512), Image.Resampling.LANCZOS).save(STORE / "icon.png")


def feature():
    image = Image.new("RGB", (2048, 1000), "#203e45")
    draw = ImageDraw.Draw(image)
    ui = ROOT / "assets/fonts/Geist-UI-560.ttf"
    reading = ROOT / "assets/fonts/Literata-Regular.ttf"
    draw.rounded_rectangle((1300, 140, 1850, 865), radius=16, fill="#ebe5d8")
    draw.text((1360, 210), "A quiet chapter", font=ImageFont.truetype(str(reading), 38), fill="#263b3c")
    for y, length in [(310, 385), (357, 355), (404, 375), (451, 280), (545, 385), (592, 375), (639, 340), (686, 385), (733, 230)]:
        draw.rounded_rectangle((1360, y, 1360 + length, y + 8), radius=4, fill="#8c9997")
    draw.text((205, 300), "Your next\nquiet chapter", font=ImageFont.truetype(str(ui), 96), spacing=16, fill="#f3eee4")
    draw.text((210, 585), "Read. Listen. Remember.", font=ImageFont.truetype(str(ui), 40), fill="#b9d4ca")
    image.resize((1024, 500), Image.Resampling.LANCZOS).save(STORE / "feature-graphic.png")


def main():
    for name, limit in [("title", 30), ("short-description", 80), ("full-description", 4000)]:
        value = (STORE / f"{name}.txt").read_text(encoding="utf-8").strip()
        if not value or len(value) > limit:
            raise ValueError(f"{name} must contain 1..{limit} characters")
    icon(); feature()
    print("Prepared 512x512 RGBA icon and 1024x500 RGB feature graphic.")


if __name__ == "__main__":
    main()
