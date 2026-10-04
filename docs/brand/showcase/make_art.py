"""Draws the showcase art: avatars, space icons and sample photos. Run: python3 make_art.py"""
import math, random
from PIL import Image, ImageDraw, ImageFilter

def lerp(a, b, t): return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))
def hexc(h): h = h.lstrip('#'); return tuple(int(h[i:i+2], 16) for i in (0, 2, 4))

def gradient(w, h, a, b, angle=45):
    img = Image.new('RGB', (w, h)); px = img.load()
    ca, sa = math.cos(math.radians(angle)), math.sin(math.radians(angle))
    span = abs(w * ca) + abs(h * sa)
    for y in range(h):
        for x in range(w):
            t = ((x * ca + y * sa) - min(0, w * ca) - min(0, h * sa)) / span
            px[x, y] = lerp(a, b, max(0, min(1, t)))
    return img

def blob(img, cx, cy, r, color, blur):
    layer = Image.new('RGBA', img.size, (0, 0, 0, 0))
    ImageDraw.Draw(layer).ellipse((cx - r, cy - r, cx + r, cy + r), fill=color + (255,))
    layer = layer.filter(ImageFilter.GaussianBlur(blur))
    img.paste(layer, (0, 0), layer)

def avatar(name, a, b, shapes, seed):
    random.seed(seed); S = 256
    img = gradient(S, S, hexc(a), hexc(b), random.choice([30, 60, 120, 210]))
    d = ImageDraw.Draw(img, 'RGBA')
    for kind, col in shapes:
        c = hexc(col) + (220,)
        if kind == 'sun': r = random.randint(46, 70); x, y = random.randint(80, 176), random.randint(60, 120); d.ellipse((x-r, y-r, x+r, y+r), fill=c)
        elif kind == 'hills':
            pts = [(0, S)] + [(x, int(S*0.62 + 22*math.sin(x/38 + seed))) for x in range(0, S+8, 8)] + [(S, S)]; d.polygon(pts, fill=c)
        elif kind == 'ring': r = 70; d.ellipse((128-r, 128-r, 128+r, 128+r), outline=c, width=26)
        elif kind == 'stripes':
            for i in range(-S, S, 40): d.polygon([(i, S), (i+20, S), (i+20+S, 0), (i+S, 0)], fill=c)
        elif kind == 'blob': blob(img, random.randint(60, 196), random.randint(60, 196), 80, hexc(col), 28); d = ImageDraw.Draw(img, 'RGBA')
        elif kind == 'tri': d.polygon([(128, 52), (212, 200), (44, 200)], fill=c)
        elif kind == 'half': d.pieslice((40, 40, 216, 216), 180, 360, fill=c)
        elif kind == 'dots':
            for gx in range(48, 224, 40):
                for gy in range(48, 224, 40): d.ellipse((gx-9, gy-9, gx+9, gy+9), fill=c)
    img.save(f'avatars/{name}.png')

import os
os.makedirs('avatars', exist_ok=True); os.makedirs('spaces', exist_ok=True); os.makedirs('photos', exist_ok=True)
avatar('maya',  '#2b2f6b', '#e0726b', [('sun', '#ffd29a'), ('hills', '#1c1f45')], 1)
avatar('theo',  '#0e3b43', '#3fa796', [('ring', '#d8f3e6')], 2)
avatar('priya', '#f2a93b', '#c2410c', [('half', '#fff4e0'), ('hills', '#7c2d12')], 3)
avatar('jules', '#1e293b', '#64748b', [('stripes', '#94a3b8'), ('sun', '#f8fafc')], 4)
avatar('kenji', '#14532d', '#84cc16', [('tri', '#ecfccb')], 5)
avatar('ada',   '#4c1d95', '#db2777', [('blob', '#fbcfe8'), ('blob', '#a78bfa')], 6)
avatar('noor',  '#0c4a6e', '#38bdf8', [('dots', '#e0f2fe')], 7)
avatar('luis',  '#422006', '#ca8a04', [('sun', '#fef3c7'), ('hills', '#1c1917')], 8)

def icon(name, a, b, draw):
    S = 256; img = gradient(S, S, hexc(a), hexc(b), 135); d = ImageDraw.Draw(img, 'RGBA'); draw(d, S); img.save(f'spaces/{name}.png')
def lantern(d, S):
    d.rounded_rectangle((98, 70, 158, 190), 18, fill=(255, 214, 140, 255)); d.rectangle((92, 60, 164, 74), fill=(40, 24, 8, 255)); d.rectangle((92, 186, 164, 198), fill=(40, 24, 8, 255))
    d.arc((108, 30, 148, 70), 180, 360, fill=(40, 24, 8, 255), width=8); d.ellipse((116, 108, 140, 150), fill=(255, 250, 230, 255))
def film(d, S):
    d.rounded_rectangle((52, 84, 204, 172), 10, fill=(20, 20, 22, 255))
    for x in range(62, 200, 22): d.rectangle((x, 90, x+10, 98), fill=(240, 230, 210, 255)); d.rectangle((x, 158, x+10, 166), fill=(240, 230, 210, 255))
    d.rectangle((70, 104, 120, 152), fill=(214, 120, 70, 255)); d.rectangle((136, 104, 186, 152), fill=(90, 140, 160, 255))
def crag(d, S):
    d.polygon([(30, 210), (110, 70), (150, 130), (180, 96), (230, 210)], fill=(245, 245, 240, 255))
    for (x, y, c) in [(108, 120, (239, 68, 68)), (150, 160, (59, 130, 246)), (176, 128, (250, 204, 21)), (90, 170, (34, 197, 94))]: d.ellipse((x-9, y-9, x+9, y+9), fill=c + (255,))
def books(d, S):
    for i, (c, h) in enumerate([((250, 250, 245), 120), ((253, 186, 116), 140), ((167, 243, 208), 110), ((191, 219, 254), 130)]):
        x = 62 + i * 34; d.rounded_rectangle((x, 196 - h, x + 26, 196), 4, fill=c + (255,))
    d.rectangle((50, 196, 206, 204), fill=(30, 30, 30, 200))
def couch(d, S):
    d.rounded_rectangle((44, 112, 212, 176), 22, fill=(250, 236, 255, 255)); d.rounded_rectangle((34, 132, 74, 196), 16, fill=(233, 213, 255, 255))
    d.rounded_rectangle((182, 132, 222, 196), 16, fill=(233, 213, 255, 255)); d.rectangle((60, 196, 72, 212), fill=(30, 10, 40, 255)); d.rectangle((184, 196, 196, 212), fill=(30, 10, 40, 255))
    d.line((128, 118, 128, 172), fill=(196, 160, 230, 255), width=6)
def owl(d, S):
    d.ellipse((150, 46, 214, 110), fill=(254, 240, 200, 255)); d.ellipse((168, 40, 228, 100), fill=(30, 27, 75, 255))
    for (x, y) in [(60, 70), (96, 46), (120, 96), (70, 130), (40, 100)]: d.ellipse((x-5, y-5, x+5, y+5), fill=(254, 249, 195, 255))
    d.rounded_rectangle((64, 150, 192, 214), 26, fill=(165, 180, 252, 255)); d.ellipse((92, 172, 112, 192), fill=(30, 27, 75, 255)); d.ellipse((144, 172, 164, 192), fill=(30, 27, 75, 255))
icon('basement', '#581c87', '#1e1b4b', couch)
icon('night-owls', '#1e1b4b', '#312e81', owl)
icon('darkroom', '#7f1d1d', '#1f1f23', film)
icon('crag', '#155e75', '#0f172a', crag)

def scene(name, w, h, sky_a, sky_b, layers, sun=None, fog=None, seed=0):
    random.seed(seed); img = gradient(w, h, hexc(sky_a), hexc(sky_b), 90)
    if sun: blob(img, int(w*sun[0]), int(h*sun[1]), int(h*sun[2]), hexc(sun[3]), int(h*0.05))
    for i, (col, base, amp, freq) in enumerate(layers):
        layer = Image.new('RGBA', (w, h), (0, 0, 0, 0)); d = ImageDraw.Draw(layer); ph = random.random() * 10
        pts = [(0, h)] + [(x, int(h*base + h*amp*math.sin(x/(w*freq) + ph) + h*amp*0.4*math.sin(x/(w*freq*0.37) + ph*2))) for x in range(0, w+6, 6)] + [(w, h)]
        d.polygon(pts, fill=hexc(col) + (255,)); img.paste(layer, (0, 0), layer)
    if fog:
        f = Image.new('RGBA', (w, h), (0, 0, 0, 0)); d = ImageDraw.Draw(f)
        for y in range(h): a = int(fog[1] * max(0, 1 - abs(y/h - fog[0]) * 3)); d.line((0, y, w, y), fill=hexc(fog[2]) + (a,))
        img.paste(f, (0, 0), f)
    return img

# A game screenshot: a lighthouse on a cliff in fog, at dusk.
g = scene('lighthouse', 1280, 720, '#1e2a4a', '#e7a87a', [('#2c3550', 0.62, 0.05, 0.11), ('#1a2036', 0.74, 0.04, 0.07), ('#0d1120', 0.86, 0.03, 0.05)], sun=(0.72, 0.58, 0.07, '#ffd9a0'), fog=(0.66, 150, '#c9c4d8'), seed=3)
d = ImageDraw.Draw(g, 'RGBA'); lx = 360
d.polygon([(lx-26, 540), (lx+26, 540), (lx+16, 300), (lx-16, 300)], fill=(236, 232, 226, 255))
for yy in (360, 420, 480): d.rectangle((lx-24, yy, lx+24, yy+18), fill=(178, 52, 52, 255))
d.rectangle((lx-22, 270, lx+22, 300), fill=(40, 40, 50, 255)); d.polygon([(lx-24, 270), (lx+24, 270), (lx, 240)], fill=(178, 52, 52, 255))
blob(g, lx, 285, 34, hexc('#fff3c4'), 14); d = ImageDraw.Draw(g, 'RGBA')
d.polygon([(lx, 285), (1280, 180), (1280, 330)], fill=(255, 243, 196, 46))
d.polygon([(lx-140, 560), (lx+180, 560), (lx+240, 720), (lx-260, 720)], fill=(10, 13, 24, 255))
d.rounded_rectangle((40, 40, 300, 92), 10, fill=(10, 12, 20, 150)); d.text((58, 56), 'BUILD 0.14.2  PLAYTEST', fill=(230, 230, 240, 255))
g.save('photos/lighthouse-fog.png')

# Film photos: a ferry at golden hour, and a hillside town.
scene('harbor-sunset', 800, 1000, '#f6c995', '#e0805a', [('#9b5d4a', 0.55, 0.01, 0.2), ('#2f5d73', 0.62, 0.006, 0.05), ('#244b5e', 0.78, 0.008, 0.03)], sun=(0.5, 0.42, 0.09, '#fff1cf'), seed=5).save('photos/harbor-sunset.png')
scene('hills', 1200, 800, '#a7c7d9', '#f2ead8', [('#7d9a6b', 0.55, 0.06, 0.12), ('#55724a', 0.68, 0.05, 0.08), ('#3b5234', 0.82, 0.04, 0.05)], fog=(0.5, 90, '#f4f1ea'), seed=9).save('photos/hills.png')
# The link preview image of the patch notes: the lantern mark, centred and without text,
# because preview cards crop a square from the middle and print the title themselves.
banner = gradient(1200, 630, hexc('#2a1406'), hexc('#c2620a'), 20); d = ImageDraw.Draw(banner, 'RGBA')
blob(banner, 600, 315, 230, hexc('#f59e0b'), 90); d = ImageDraw.Draw(banner, 'RGBA')
lx, ly = 600, 330  # centred: link preview cards crop a square from the middle
d.rounded_rectangle((lx-70, ly-110, lx+70, ly+120), 40, fill=(255, 214, 140, 255)); d.rectangle((lx-84, ly-130, lx+84, ly-104), fill=(40, 24, 8, 255)); d.rectangle((lx-84, ly+114, lx+84, ly+140), fill=(40, 24, 8, 255))
d.arc((lx-50, ly-200, lx+50, ly-100), 180, 360, fill=(40, 24, 8, 255), width=16); d.ellipse((lx-30, ly-40, lx+30, ly+50), fill=(255, 250, 230, 255))
banner.save('photos/patch-notes.png')
print('done')
