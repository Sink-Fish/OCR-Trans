"""Render a few synthetic screenshots into testdata/ (uses macOS system fonts)."""
import sys
from PIL import Image, ImageDraw, ImageFont

OUT = sys.argv[1] if len(sys.argv) > 1 else "testdata"
F = "/System/Library/Fonts/Helvetica.ttc"
JP = "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc"


def wrap(draw, text, font, width):
    words, lines, cur = text.split(), [], ""
    for w in words:
        t = (cur + " " + w).strip()
        if draw.textlength(t, font=font) > width:
            lines.append(cur)
            cur = w
        else:
            cur = t
    lines.append(cur)
    return lines


def doc(path, paras, size=28, bg="white", fg="#222", w=1400, title=None):
    font = ImageFont.truetype(F, size)
    tfont = ImageFont.truetype(F, int(size * 1.6))
    img = Image.new("RGB", (w, 2000), bg)
    d = ImageDraw.Draw(img)
    y = 40
    if title:
        d.text((60, y), title, font=tfont, fill=fg)
        y += int(size * 3)
    for p in paras:
        for line in wrap(d, p, font, w - 120):
            d.text((60, y), line, font=font, fill=fg)
            y += int(size * 1.45)
        y += size
    img.crop((0, 0, w, y + 20)).save(path)


doc(f"{OUT}/en_article.png", [
    "A reader selects the narrow column near the edge of the window. OCR Trans groups the lines in reading order "
    "so that the first line of the next column stays separate from the sentence above it.",
    "The blue note stays pinned while the reader switches between the captured words and the translation. "
    "After checking the last line, the reader saves a copy and closes the note.",
    "Please make sure the application has permission to record the screen before you take your first screenshot.",
], title="How screenshot translation works")

doc(f"{OUT}/en_dark_ui.png", [
    "Settings",
    "Automatically check for updates every week.",
    "Show notifications when a download finishes.",
    "Your subscription will renew on March 3. You can cancel at any time from the billing page.",
    "Error: The file could not be saved because the disk is full.",
], size=22, bg="#1e1f22", fg="#e6e6e6", w=900)

doc(f"{OUT}/en_small.png", [
    "By clicking Continue, you agree to our Terms of Service and acknowledge that you have read our Privacy Policy.",
    "Free shipping on orders over $50. Returns are accepted within 30 days of delivery.",
], size=15, w=700)

# Japanese
img = Image.new("RGB", (1100, 330), "#fafaf5")
d = ImageDraw.Draw(img)
jf = ImageFont.truetype(JP, 30)
for i, t in enumerate([
    "画面の一部を選択すると、文字を認識して翻訳します。",
    "この機能はオフラインで動作し、データは外部に送信されません。",
    "設定画面からショートカットキーを変更できます。",
    "ダウンロードが完了しました。",
]):
    d.text((50, 40 + i * 70), t, font=jf, fill="#333")
img.save(f"{OUT}/ja_ui.png")
print("ok")

# complex backgrounds for inpainting
import random
W, H = 1000, 300
g = Image.new("RGB", (W, H))
px = g.load()
for y in range(H):
    for x in range(W):
        px[x, y] = (40 + x * 150 // W, 90 + y * 100 // H, 200 - x * 120 // W)
d = ImageDraw.Draw(g)
f = ImageFont.truetype(F, 34)
d.text((60, 50), "Welcome back! Your files are synced.", font=f, fill="white")
d.text((60, 120), "Tap here to open the latest report.", font=f, fill="white")
d.text((60, 190), "Storage almost full", font=ImageFont.truetype(F, 26), fill="#ffe08a")
g.save(f"{OUT}/en_gradient.png")

random.seed(1)
t = Image.new("RGB", (W, H))
px = t.load()
for y in range(H):
    for x in range(W):
        n = random.randint(-40, 40)
        base = 120 + int(60 * ((x // 25 + y // 25) % 2))
        px[x, y] = (max(0, min(255, base + n)), max(0, min(255, base - 20 + n)), max(0, min(255, base - 50 + n)))
d = ImageDraw.Draw(t)
d.text((60, 60), "Limited offer: buy one, get one free", font=f, fill="black")
d.text((60, 150), "Ends Sunday at midnight", font=f, fill="black")
t.save(f"{OUT}/en_texture.png")
print("ok2")

# Full-size synthetic fixture for the overlay keyboard checks.
screen = Image.new("RGB", (3456, 2160), "#e9edf2")
draw = ImageDraw.Draw(screen)
font = ImageFont.truetype(JP, 54)
for row, caption in enumerate(["框选一段文字", "识别结果可以复制", "按回车确认，按退出键取消"]):
    y = 340 + row * 360
    draw.rectangle((260, y, 3150, y + 230), fill="white")
    draw.text((330, y + 72), caption, font=font, fill="#233448")
screen.save(f"{OUT}/screen_full_zh.png")
