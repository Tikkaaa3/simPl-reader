"""Deterministic, self-authored PDF with an empty user password and no copy grant.

Uses PDF Standard Security revision 2 solely to test permission enforcement.
No third-party document or Python dependency is needed.
"""
from hashlib import md5
from pathlib import Path
from struct import pack

PADDING = bytes.fromhex('28bf4e5e4e758a4164004e56fffa01082e2e00b6d0683e802f0ca9fe6453697a')


def rc4(key, data):
    table = list(range(256))
    j = 0
    for i in range(256):
        j = (j + table[i] + key[i % len(key)]) % 256
        table[i], table[j] = table[j], table[i]
    i = j = 0
    result = bytearray()
    for byte in data:
        i = (i + 1) % 256
        j = (j + table[i]) % 256
        table[i], table[j] = table[j], table[i]
        result.append(byte ^ table[(table[i] + table[j]) % 256])
    return bytes(result)


owner_password = (b'simPl fixture owner' + PADDING)[:32]
owner = rc4(md5(owner_password).digest()[:5], PADDING)
permissions = -20
identifier = md5(b'simPl M5 copy restriction fixture').digest()
key = md5(PADDING + owner + pack('<i', permissions) + identifier).digest()[:5]
user = rc4(key, PADDING)
content = b'BT /F1 18 Tf 20 150 Td (Copy-restricted Lighthouse) Tj ET'
object_key = md5(key + (4).to_bytes(3, 'little') + b'\x00\x00').digest()[:10]
encrypted = rc4(object_key, content)
objects = [
    b'<</Type/Catalog/Pages 2 0 R>>',
    b'<</Type/Pages/Kids[3 0 R]/Count 1>>',
    b'<</Type/Page/Parent 2 0 R/MediaBox[0 0 300 200]/Contents 4 0 R/Resources<</Font<</F1 5 0 R>>>>>>',
    f'<</Length {len(encrypted)}>>\nstream\n'.encode() + encrypted + b'\nendstream',
    b'<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>',
    f'<</Filter/Standard/V 1/R 2/Length 40/O<{owner.hex()}>/U<{user.hex()}>/P {permissions}>>'.encode(),
]
pdf = bytearray(b'%PDF-1.4\n')
offsets = []
for index, body in enumerate(objects, 1):
    offsets.append(len(pdf))
    pdf.extend(f'{index} 0 obj\n'.encode() + body + b'\nendobj\n')
xref = len(pdf)
pdf.extend(f'xref\n0 {len(objects) + 1}\n0000000000 65535 f \n'.encode())
for offset in offsets:
    pdf.extend(f'{offset:010} 00000 n \n'.encode())
pdf.extend(f'trailer\n<</Root 1 0 R/Size {len(objects) + 1}/Encrypt 6 0 R/ID[<{identifier.hex()}><{identifier.hex()}>]>>\nstartxref\n{xref}\n%%EOF\n'.encode())
Path(__file__).with_name('copy-restricted.pdf').write_bytes(pdf)
