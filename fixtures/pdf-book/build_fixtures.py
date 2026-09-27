"""Authored PDF Book regression inputs. Requires reportlab and pypdf."""
from pathlib import Path
from reportlab.pdfgen import canvas
from reportlab.lib.utils import ImageReader
from pypdf import PdfReader, PdfWriter
from pypdf.constants import UserAccessPermissions

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

writer = PdfWriter()
for page in PdfReader(out / "prose.pdf").pages:
    writer.add_page(page)
writer.encrypt(user_password="", owner_password="fixture-owner", permissions_flag=UserAccessPermissions.PRINT)
with (out / "restricted.pdf").open("wb") as stream:
    writer.write(stream)
print(out)
