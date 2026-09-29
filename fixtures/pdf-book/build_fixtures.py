"""Authored PDF Book regression inputs. Requires reportlab and pypdf."""
from pathlib import Path
from reportlab.pdfgen import canvas
from reportlab.lib.utils import ImageReader
from pypdf import PdfReader, PdfWriter
from pypdf.constants import PageLabelStyle, UserAccessPermissions

root = Path(__file__).resolve().parents[2]
out = root / "target" / "book-milestone3" / "fixtures"
out.mkdir(parents=True, exist_ok=True)
image = root / "fixtures" / "reader-workload" / "assets" / "images" / "reader-sample.png"

def new(name):
    pdf = canvas.Canvas(str(out / name), pagesize=(540, 720))
    pdf.setTitle("A quiet reading journey")
    pdf.setAuthor("simPl regression fixture")
    return pdf

def margin(pdf, page):
    pdf.setFont("Helvetica", 9)
    pdf.drawString(55, 684, "A QUIET READING JOURNEY")
    pdf.drawRightString(485, 28, f"Page {page}")

pdf = new("prose.pdf")
for page in range(1, 5):
    margin(pdf, page)
    pdf.setFont("Times-Bold", 22)
    pdf.drawString(55, 620, f"Chapter {page}")
    pdf.setFont("Times-Roman", 13)
    lines = [
        "A book gives its reader room to think. A paragraph can continue",
        "over several printed lines while keeping its original meaning.",
        "Our reading journey follows the text in its source order.",
    ]
    for i, line in enumerate(lines):
        pdf.drawString(55, 574 - 19*i, line)
    lines = [
        "A second paragraph discusses read-",
        "ing without discarding a meaningful well-",
        "known compound. The word reading gives evidence for cleanup.",
    ]
    for i, line in enumerate(lines):
        pdf.drawString(55, 486 - 19*i, line)
    pdf.setFont("Times-Italic", 13)
    pdf.drawString(55, 396, "Punctuation stays: commas, colons; and a final question?")
    pdf.showPage()
pdf.drawImage(ImageReader(image), 80, 220, 380, 253)
pdf.showPage()
pdf.save()

pdf = new("columns.pdf")
for x, label in [(45, "LEFT"), (300, "RIGHT")]:
    pdf.setFont("Times-Roman", 12)
    for i in range(18):
        pdf.drawString(x, 630 - 22*i, f"{label} column, sentence {i+1}.")
pdf.showPage(); pdf.save()

pdf = new("scanned.pdf")
pdf.drawImage(ImageReader(image), 80, 220, 380, 253)
pdf.showPage(); pdf.save()

pdf = new("layout-source.pdf")
pdf.setFont("Times-Bold", 28)
pdf.drawCentredString(270, 435, "A BOOK OF MYTHS")
pdf.setFont("Times-Italic", 14)
pdf.drawCentredString(270, 395, "A short illustrated edition")
pdf.setFont("Times-Roman", 11)
pdf.drawCentredString(270, 24, "i")
pdf.showPage()

pdf.setFont("Times-Bold", 22)
pdf.drawCentredString(270, 628, "Contents")
pdf.setFont("Times-Roman", 13)
for title, folio, x, y in [
    ("Introduction", "vii", 75, 570),
    ("The first myth", "1", 90, 535),
    ("The second myth", "25", 90, 500),
]:
    pdf.drawString(x, y, title)
    pdf.drawRightString(475, y, folio)
    if title == "The first myth":
        pdf.linkRect("", "myths", (x - 2, y - 3, x + 110, y + 15), relative=0, thickness=0)
pdf.setFont("Times-Roman", 11)
pdf.drawCentredString(270, 24, "ii")
pdf.showPage()

pdf.bookmarkPage("myths")
pdf.setFont("Times-Bold", 16)
pdf.drawString(70, 620, "Myth #38 The first claim")
pdf.setFont("Times-Italic", 14)
pdf.drawString(70, 580, "Myth #39 The second claim")
pdf.setFont("Times-Roman", 13)
pdf.drawString(70, 530, "Ordinary prose can wrap across a printed line")
pdf.drawString(70, 510, "without losing its selectable reading order.")
pdf.setFont("Times-Roman", 11)
pdf.drawCentredString(270, 24, "iii")
pdf.showPage()

pdf.setFont("Times-Roman", 12)
for row, y in enumerate([610, 570, 530, 490]):
    for col, x in enumerate([60, 225, 390]):
        pdf.drawString(x, y, f"Cell {row + 1}-{col + 1}")
pdf.showPage()
pdf.save()
writer = PdfWriter()
for page in PdfReader(out / "layout-source.pdf").pages:
    writer.add_page(page)
writer.set_page_label(0, 2, style=PageLabelStyle.LOWERCASE_ROMAN, start=1)
writer.set_page_label(3, 3, style=PageLabelStyle.DECIMAL, start=1)
with (out / "layout.pdf").open("wb") as stream:
    writer.write(stream)
(out / "layout-source.pdf").unlink()

writer = PdfWriter()
for page in PdfReader(out / "prose.pdf").pages:
    writer.add_page(page)
writer.encrypt(user_password="", owner_password="fixture-owner", permissions_flag=UserAccessPermissions.PRINT)
with (out / "restricted.pdf").open("wb") as stream:
    writer.write(stream)
print(out)
