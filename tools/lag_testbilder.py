"""Lager et kunstig testsett for Bildesamler med kjent fasit (fasit.csv).

Bruk: python lag_testbilder.py <utmappe>
"""
import csv
import os
import random
import subprocess
import sys
import time
from datetime import datetime

from PIL import Image, ImageDraw, ImageFilter

UT = sys.argv[1]
INN = os.path.join(UT, "innboks")  # det appen skal sortere
os.makedirs(INN, exist_ok=True)
NAA = time.time()
rader = []


def motiv(seed, w=1600, h=1200):
    """Unikt bilde: gradient + tilfeldige figurer, slik at perseptuell hash skiller dem."""
    r = random.Random(seed)
    img = Image.new("RGB", (w, h))
    d = ImageDraw.Draw(img)
    c1 = [r.randint(0, 255) for _ in range(3)]
    c2 = [r.randint(0, 255) for _ in range(3)]
    for y in range(h):
        t = y / h
        d.line([(0, y), (w, y)], fill=tuple(int(a + (b - a) * t) for a, b in zip(c1, c2)))
    for _ in range(12):
        x0, y0 = r.randint(0, w), r.randint(0, h)
        s = r.randint(80, 500)
        farge = tuple(r.randint(0, 255) for _ in range(3))
        if r.random() < 0.5:
            d.ellipse([x0, y0, x0 + s, y0 + s], fill=farge)
        else:
            d.rectangle([x0, y0, x0 + s, y0 + s // 2], fill=farge)
    d.text((40, 40), f"motiv {seed}", fill=(255, 255, 255))
    return img


def exif(dato=None, make="Canon", model="EOS 80D", gps=False):
    e = Image.Exif()
    if make:
        e[0x010F] = make
        e[0x0110] = model
    if dato:
        e.get_ifd(0x8769)[0x9003] = dato.strftime("%Y:%m:%d %H:%M:%S")
    if gps:
        e.get_ifd(0x8825).update({1: "N", 2: (59.0, 54.0, 0.0), 3: "E", 4: (10.0, 45.0, 0.0)})
    return e


def lagre(img, navn, *, e=None, kvalitet=95, mtime=None, **fasit):
    sti = os.path.join(INN, navn)
    os.makedirs(os.path.dirname(sti), exist_ok=True)
    if navn.lower().endswith(".png"):
        img.save(sti)
    else:
        img.save(sti, quality=kvalitet, exif=e if e is not None else Image.Exif())
    if mtime is not None:
        os.utime(sti, (mtime, mtime))
    rader.append({"fil": os.path.relpath(sti, INN).replace(os.sep, "/"), **fasit})


def ts(dt):
    return dt.timestamp()


def mål(dt, typ="Bilder"):
    return f"{typ}/{dt:%Y}/{dt:%m}"


nr = 0


def ny_seed():
    global nr
    nr += 1
    return nr


# 1) Rene originaler med riktig EXIF, spredt over år og måneder
for i in range(20):
    dt = datetime(2012 + i % 12, 1 + (i * 5) % 12, 1 + i % 27, 10 + i % 9, 15)
    s = ny_seed()
    lagre(motiv(s), f"kamera/IMG_{1000 + i}.JPG", e=exif(dt, gps=i % 3 == 0), mtime=ts(dt),
          scenario="original", gruppe=s, forventet_dato=dt.isoformat(), forventet_mål=mål(dt), beholdes="ja")

# 2) Eksakte kopier (samme bytes, nytt navn, falsk fildato = nedlastingsdag)
for i in range(5):
    src = os.path.join(INN, rader[i]["fil"])
    navn = f"onedrive_kopi/IMG_{1000 + i} (1).JPG"
    sti = os.path.join(INN, navn)
    os.makedirs(os.path.dirname(sti), exist_ok=True)
    with open(src, "rb") as a, open(sti, "wb") as b:
        b.write(a.read())
    os.utime(sti, (NAA, NAA))
    rader.append({"fil": navn, "scenario": "eksakt kopi", "gruppe": rader[i]["gruppe"],
                  "forventet_dato": rader[i]["forventet_dato"], "forventet_mål": "Duplikater", "beholdes": "nei"})

# 3) WhatsApp-kopier: nedskalert, hardt komprimert, EXIF fjernet, dato bare i filnavn
for i in range(5, 10):
    o = rader[i]
    dt = datetime.fromisoformat(o["forventet_dato"])
    img = motiv(o["gruppe"]).resize((800, 600))
    lagre(img, f"whatsapp/IMG-{dt:%Y%m%d}-WA{i:04d}.jpg", kvalitet=55, mtime=NAA,
          scenario="whatsapp-kopi (dårligere, uten EXIF)", gruppe=o["gruppe"],
          forventet_dato=dt.isoformat(), forventet_mål="Duplikater", beholdes="nei")

# 4) Bedre kopi uten metadata vs. dårligere med metadata: originalen med riktig dato skal vinne
for i in range(10, 13):
    o = rader[i]
    img = motiv(o["gruppe"])
    lagre(img, f"google_foto/ukjent_{i}.jpg", kvalitet=98, mtime=NAA,
          scenario="full kvalitet men uten EXIF (original med EXIF vinner)", gruppe=o["gruppe"],
          forventet_dato=o["forventet_dato"], forventet_mål="Duplikater", beholdes="nei")

# 5) Uten EXIF, dato i filnavnet (skjermbilder og Android-navn)
for i in range(6):
    dt = datetime(2020 + i % 5, 2 + i, 10, 14, 30, 5)
    s = ny_seed()
    navn = f"mobil/Screenshot_{dt:%Y%m%d-%H%M%S}.png" if i % 2 else f"mobil/IMG_{dt:%Y%m%d_%H%M%S}.jpg"
    lagre(motiv(s, 1080, 2340), navn, mtime=NAA, scenario="dato bare i filnavn", gruppe=s,
          forventet_dato=dt.isoformat(), forventet_mål=mål(dt), beholdes="ja")

# 6) Ingen dato noe sted (fildato = i dag): skal til Uten dato
for i in range(4):
    s = ny_seed()
    lagre(motiv(s), f"diverse/bilde{i + 1}.jpg", mtime=NAA, scenario="ingen dato", gruppe=s,
          forventet_dato="", forventet_mål="Uten dato", beholdes="ja")

# 7) Kamera med nullstilt klokke og fremtidsdato: datoen lyver
for i, falsk in enumerate([datetime(2000, 1, 1, 0, 0, 0), datetime(1970, 1, 1, 0, 0, 0), datetime(2031, 5, 5, 12, 0)]):
    s = ny_seed()
    lagre(motiv(s), f"kamera/DSC_{9000 + i}.JPG", e=exif(falsk, "Sony", "DSC-W800"), mtime=NAA,
          scenario=f"falsk EXIF-dato ({falsk:%Y-%m-%d})", gruppe=s, forventet_dato="",
          forventet_mål="Uten dato", beholdes="ja")

# 8) EXIF og filnavn uenige: EXIF vinner, men skal flagges
dt = datetime(2018, 8, 12, 9, 0)
s = ny_seed()
lagre(motiv(s), "mobil/IMG_20150301_120000.jpg", e=exif(dt, "samsung", "SM-G960F"), mtime=NAA,
      scenario="EXIF og filnavn uenige (flagg)", gruppe=s, forventet_dato=dt.isoformat(),
      forventet_mål=mål(dt), beholdes="ja")

# 9) Serieopptak: like, men ikke duplikater (små forskjeller)
base = motiv(ny_seed())
dt = datetime(2022, 6, 23, 21, 10, 0)
for i in range(3):
    b = base.copy()
    ImageDraw.Draw(b).ellipse([300 + i * 120, 500, 500 + i * 120, 700], fill=(255, 255, 0))
    t = dt.replace(second=i)
    lagre(b, f"kamera/BURST_{i + 1}.JPG", e=exif(t, "Apple", "iPhone 13"), mtime=ts(t),
          scenario="serieopptak (ikke duplikat)", gruppe=f"serie-{nr}-{i}", forventet_dato=t.isoformat(),
          forventet_mål=mål(t), beholdes="ja")

# 10) Redigert versjon (beskåret + filter): beholdes ved siden av originalen
o = rader[15]
dt = datetime.fromisoformat(o["forventet_dato"])
red = motiv(o["gruppe"]).crop((200, 150, 1400, 1050)).filter(ImageFilter.SMOOTH)
lagre(red, f"kamera/IMG_{1015}_redigert.JPG", e=exif(dt), mtime=NAA, scenario="redigert versjon (beholdes)",
      gruppe=f"{o['gruppe']}-redigert", forventet_dato=dt.isoformat(), forventet_mål=mål(dt), beholdes="ja")

# 11) Norske tegn og mellomrom i filnavn
dt = datetime(2016, 12, 24, 18, 0)
s = ny_seed()
lagre(motiv(s), "familie/Julaften på hytta æøå.jpg", e=exif(dt, "NIKON", "D3500"), mtime=ts(dt),
      scenario="norske tegn i filnavn", gruppe=s, forventet_dato=dt.isoformat(), forventet_mål=mål(dt), beholdes="ja")

# 12) Videoer med creation_time, pluss en eksakt kopi
os.makedirs(os.path.join(INN, "video"), exist_ok=True)
for i, dt in enumerate([datetime(2019, 7, 4, 13, 0), datetime(2023, 3, 17, 8, 45)]):
    navn = f"video/VID_{dt:%Y%m%d_%H%M%S}.mp4"
    sti = os.path.join(INN, navn)
    subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-f", "lavfi", "-i",
                    f"testsrc=duration=2:size=640x360:rate=25", "-vf", f"hue=h={i * 120}",
                    "-metadata", f"creation_time={dt:%Y-%m-%dT%H:%M:%S}Z", "-pix_fmt", "yuv420p", sti], check=True)
    os.utime(sti, (NAA, NAA))
    rader.append({"fil": navn, "scenario": "video", "gruppe": f"video-{i}", "forventet_dato": dt.isoformat(),
                  "forventet_mål": mål(dt, "Videoer"), "beholdes": "ja"})
kopi = "video/VID_20190704_130000 (kopi).mp4"
with open(os.path.join(INN, rader[-2]["fil"]), "rb") as a, open(os.path.join(INN, kopi), "wb") as b:
    b.write(a.read())
rader.append({"fil": kopi, "scenario": "video, eksakt kopi", "gruppe": "video-0",
              "forventet_dato": rader[-2]["forventet_dato"], "forventet_mål": "Duplikater", "beholdes": "nei"})

with open(os.path.join(UT, "fasit.csv"), "w", newline="", encoding="utf-8") as f:
    w = csv.DictWriter(f, fieldnames=["fil", "scenario", "gruppe", "forventet_dato", "forventet_mål", "beholdes"])
    w.writeheader()
    w.writerows(rader)
print(f"{len(rader)} filer laget i {INN}")
