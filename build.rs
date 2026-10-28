// Embeds the app icon and version info into the .exe.

use std::path::Path;

include!("src/icon.rs");

/// Builds a .ico file with uncompressed 32-bit BMP images.
fn build_ico(sizes: &[u32]) -> Vec<u8> {
    let images: Vec<Vec<u8>> = sizes
        .iter()
        .map(|&size| {
            let rgba = render_icon(size);
            let mask_row = size.div_ceil(32) * 4;
            let mut bmp = Vec::new();
            for v in [40u32, size, size * 2] {
                bmp.extend_from_slice(&v.to_le_bytes());
            }
            bmp.extend_from_slice(&1u16.to_le_bytes()); // planes
            bmp.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
            bmp.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
            bmp.extend_from_slice(&(size * size * 4 + mask_row * size).to_le_bytes());
            bmp.extend_from_slice(&[0u8; 16]);
            for row in (0..size).rev() {
                for col in 0..size {
                    let i = ((row * size + col) * 4) as usize;
                    bmp.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
                }
            }
            bmp.resize(bmp.len() + (mask_row * size) as usize, 0);
            bmp
        })
        .collect();

    let mut ico = Vec::new();
    ico.extend_from_slice(&[0, 0, 1, 0]);
    ico.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (&size, img) in sizes.iter().zip(&images) {
        let dim = if size >= 256 { 0 } else { size as u8 };
        ico.extend_from_slice(&[dim, dim, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&(img.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += img.len() as u32;
    }
    for img in images {
        ico.extend_from_slice(&img);
    }
    ico
}

fn main() {
    println!("cargo:rerun-if-changed=src/icon.rs");
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let ico_path = Path::new(&out_dir).join("app.ico");
    std::fs::write(&ico_path, build_ico(&[16, 24, 32, 48, 64, 256])).unwrap();

    let version = env!("CARGO_PKG_VERSION");
    let parts: Vec<&str> = version.split('.').collect();
    let comma = format!("{},{},{},0", parts[0], parts[1], parts[2]);
    let rc = format!(
        r#"1 ICON "{ico}"
1 VERSIONINFO
FILEVERSION {comma}
PRODUCTVERSION {comma}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "FileDescription", "RDP Manager"
      VALUE "ProductName", "RDP Manager"
      VALUE "FileVersion", "{version}"
      VALUE "ProductVersion", "{version}"
      VALUE "OriginalFilename", "rdp-manager.exe"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        ico = ico_path.display().to_string().replace('\\', "\\\\"),
    );
    let rc_path = Path::new(&out_dir).join("app.rc");
    std::fs::write(&rc_path, rc).unwrap();
    embed_resource::compile(&rc_path, embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
