#!/usr/bin/env python3
import os
import subprocess
import shutil
from PIL import Image, ImageDraw

def create_mac_squircle(src_img_path, output_path):
    """
    Creates a macOS Big Sur style squircle icon.
    Canvas: 1024x1024
    Squircle: 824x824, centered (margin of 100px)
    Corner radius: ~180px (typical for Big Sur app icons)
    """
    src = Image.open(src_img_path).convert("RGBA")
    
    # 1. Create a transparent canvas
    canvas = Image.new("RGBA", (1024, 1024), (0, 0, 0, 0))
    
    # 2. Resize source image to the squircle bounding box (824x824)
    squircle_size = 824
    content = src.resize((squircle_size, squircle_size), Image.Resampling.LANCZOS)
    
    # 3. Create a rounded rectangle mask for the squircle
    mask = Image.new("L", (squircle_size, squircle_size), 0)
    draw = ImageDraw.Draw(mask)
    
    # corner radius of 180 is typical for 824x824 macOS icon
    radius = 180
    draw.rounded_rectangle((0, 0, squircle_size, squircle_size), radius=radius, fill=255)
    
    # Apply the mask to the content
    masked_content = Image.new("RGBA", (squircle_size, squircle_size), (0, 0, 0, 0))
    masked_content.paste(content, (0, 0), mask=mask)
    
    # 4. Paste onto the 1024x1024 canvas, centered
    offset = (1024 - squircle_size) // 2
    canvas.paste(masked_content, (offset, offset))
    
    canvas.save(output_path, "PNG")
    print(f"Created macOS squircle PNG at: {output_path}")

def generate_icns(png_1024_path, output_icns_path):
    """
    Generates a macOS .icns file using the built-in sips and iconutil tools.
    """
    iconset_dir = "temp_icon.iconset"
    os.makedirs(iconset_dir, exist_ok=True)
    
    # Define macOS icon sizes and names
    sizes = [
        (16, "icon_16x16.png"),
        (32, "icon_16x16@2x.png"),
        (32, "icon_32x32.png"),
        (64, "icon_32x32@2x.png"),
        (128, "icon_128x128.png"),
        (256, "icon_128x128@2x.png"),
        (256, "icon_256x256.png"),
        (512, "icon_256x256@2x.png"),
        (512, "icon_512x512.png"),
        (1024, "icon_512x512@2x.png"),
    ]
    
    img = Image.open(png_1024_path)
    for size, name in sizes:
        resized = img.resize((size, size), Image.Resampling.LANCZOS)
        resized.save(os.path.join(iconset_dir, name))
        
    # Compile using iconutil
    subprocess.run(["iconutil", "-c", "icns", iconset_dir, "-o", output_icns_path], check=True)
    
    # Clean up temp files
    shutil.rmtree(iconset_dir)
    print(f"Successfully generated macOS ICNS at: {output_icns_path}")

def generate_ico(src_img_path, output_ico_path):
    """
    Generates a Windows .ico file with multiple resolutions.
    """
    src = Image.open(src_img_path).convert("RGBA")
    src = src.resize((1024, 1024), Image.Resampling.LANCZOS)
    
    # Let's apply a subtle rounding to the Windows icon to make it look premium (e.g. radius=80 for 1024x1024)
    # Windows doesn't require a strict margin, so we can make it full bleed with small rounded corners.
    mask = Image.new("L", (1024, 1024), 0)
    draw = ImageDraw.Draw(mask)
    draw.rounded_rectangle((0, 0, 1024, 1024), radius=100, fill=255)
    
    rounded_src = Image.new("RGBA", (1024, 1024), (0, 0, 0, 0))
    rounded_src.paste(src, (0, 0), mask=mask)
    
    sizes = [(16, 16), (32, 32), (48, 48), (256, 256)]
    rounded_src.save(output_ico_path, format="ICO", sizes=sizes)
    print(f"Successfully generated Windows ICO at: {output_ico_path}")

if __name__ == "__main__":
    src_png = "assets/logo.png"
    
    assets_dir = "assets"
    os.makedirs(assets_dir, exist_ok=True)
    
    # 1. Create temporary squircle PNG for macOS
    temp_squircle_png = "assets/temp_squircle.png"
    create_mac_squircle(src_png, temp_squircle_png)
    
    # 2. Convert to macOS .icns
    generate_icns(temp_squircle_png, "assets/icon.icns")
    
    # Clean up temp squircle png
    if os.path.exists(temp_squircle_png):
        os.remove(temp_squircle_png)
        
    # 3. Create Windows .ico
    generate_ico(src_png, "assets/icon.ico")
    
    print("All icons generated successfully!")
