#![allow(dead_code)]
use hmi_compiler::binary::{directory_crc, main_crc, page_crc, put_u32};
use hmi_compiler::container::{BACKUP_START, DATA_START};

fn record(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}

fn object(name: &str, kind: u8, id: u8) -> Vec<u8> {
    let mut attrs: Vec<(&str, Vec<u8>)> = vec![
        ("type", vec![kind]),
        ("id", vec![id]),
        ("objname", name.as_bytes().to_vec()),
        ("vscope", vec![0]),
        ("x", 10u16.to_le_bytes().to_vec()),
        ("y", 20u16.to_le_bytes().to_vec()),
        ("w", 100u16.to_le_bytes().to_vec()),
        ("h", 40u16.to_le_bytes().to_vec()),
        ("endx", 109u16.to_le_bytes().to_vec()),
        ("endy", 59u16.to_le_bytes().to_vec()),
        ("font", vec![0]),
        ("bco", 65535u16.to_le_bytes().to_vec()),
        ("sta", vec![1]),
        ("opaque", vec![0xFE, 0xED, 0xAB]),
    ];
    if kind == 98 {
        attrs.extend([
            ("txt", "返回".as_bytes().to_vec()),
            ("txt_maxl", 12u16.to_le_bytes().to_vec()),
            ("val", vec![0]),
        ]);
    }
    if kind == 52 {
        attrs.push(("val", (-7i32).to_le_bytes().to_vec()));
    }
    let mut out = Vec::new();
    record(&mut out, format!("att-{}", attrs.len()).as_bytes());
    for (name, value) in attrs {
        let mut field = vec![0; 16];
        field[..name.len()].copy_from_slice(name.as_bytes());
        field[15] = 0xA7; // NUL 后的未知名称字段字节也必须保留。
        field.extend(value);
        record(&mut out, &field);
    }
    record(&mut out, b"unrecognized-metadata");
    if kind == 121 {
        record(&mut out, b"codesload-0");
        record(&mut out, b"codesloadend-0");
        record(&mut out, b"codesunload-0");
    }
    record(&mut out, b"codesdown-0");
    record(
        &mut out,
        if kind == 98 {
            b"codesup-2"
        } else {
            b"codesup-0"
        },
    );
    if kind == 98 {
        record(&mut out, b"// keep spaces  ");
        record(&mut out, b"dp=0");
    }
    record(&mut out, b"");
    out
}

pub fn page() -> Vec<u8> {
    page_named("page0")
}

pub fn page_named(name: &str) -> Vec<u8> {
    let objects = [
        object(name, 121, 0),
        object("b0", 98, 1),
        object("va0", 52, 2),
    ];
    let mut page = vec![0; 56 + objects.len() * 12];
    put_u32(&mut page, 8, 56).unwrap();
    put_u32(&mut page, 12, objects.len()).unwrap();
    page[21] = 0x55;
    page[22] = 0x21;
    page[24..24 + name.len()].copy_from_slice(name.as_bytes());
    for (i, bytes) in objects.iter().enumerate() {
        let offset = page.len() - 56;
        put_u32(&mut page, 56 + i * 12, offset).unwrap();
        put_u32(&mut page, 60 + i * 12, bytes.len()).unwrap();
        put_u32(&mut page, 64 + i * 12, 0xABCD + i).unwrap();
        page.extend_from_slice(bytes);
    }
    let length = page.len();
    put_u32(&mut page, 4, length).unwrap();
    let crc = page_crc(&page).unwrap();
    page[..4].copy_from_slice(&crc.to_le_bytes());
    page
}

pub fn fixture() -> Vec<u8> {
    fixture_with_pages(1)
}

pub fn fixture_with_pages(count: usize) -> Vec<u8> {
    let mut main = vec![0; 96 + (count + 2) * 16];
    put_u32(&mut main, 4, 96).unwrap();
    main[13] = 0x18;
    put_u32(&mut main, 24, 96).unwrap();
    put_u32(&mut main, 28, count + 2).unwrap();
    let references: Vec<(String, String)> =
        [("i".into(), "0.i".into()), ("zi".into(), "0.zi".into())]
            .into_iter()
            .chain((0..count).map(|i| ("pa".into(), format!("{}.pa", 39 + i))))
            .collect();
    for (i, (kind, resource)) in references.iter().enumerate() {
        let start = 96 + i * 16;
        main[start..start + kind.len()].copy_from_slice(kind.as_bytes());
        main[start + 8..start + 8 + resource.len()].copy_from_slice(resource.as_bytes());
    }
    let crc = main_crc(&main).unwrap();
    main[..4].copy_from_slice(&crc.to_le_bytes());
    let mut resources: Vec<(String, Vec<u8>, bool)> =
        vec![("old.bin".into(), vec![0xAA; 11], true)];
    for i in 0..count {
        resources.push((
            format!("{}.pa", 39 + i),
            page_named(&format!("page{i}")),
            false,
        ));
    }
    resources.extend([
        ("main.HMI".into(), main, false),
        (
            "Program.s".into(),
            "// 全局代码\r\nint sys0=0\r\npage 0\r\n"
                .as_bytes()
                .to_vec(),
            false,
        ),
        ("0.i".into(), vec![0x55; 26], false),
        ("0.zi".into(), vec![0x33; 48], false),
    ]);
    let mut out = vec![0x7B; DATA_START];
    out[DATA_START - 8..].copy_from_slice(b"ver21234");
    for base in [0, BACKUP_START] {
        put_u32(&mut out, base, resources.len()).unwrap();
    }
    for (i, (name, data, deleted)) in resources.iter().enumerate() {
        let start = out.len();
        for base in [0, BACKUP_START] {
            let index = base + 4 + i * 28;
            out[index..index + 16].fill(0);
            out[index..index + name.len()].copy_from_slice(name.as_bytes());
            put_u32(&mut out, index + 16, start).unwrap();
            put_u32(&mut out, index + 20, data.len()).unwrap();
            out[index + 24] = u8::from(*deleted);
            out[index + 25..index + 28].copy_from_slice(&[0x12, 0xFE, 0x67]);
        }
        out.extend_from_slice(data);
    }
    fix_directory(&mut out);
    out
}

pub fn fix_directory(bytes: &mut [u8]) {
    let count = hmi_compiler::binary::usize32(bytes, 0).unwrap();
    for base in [0, BACKUP_START] {
        let end = base + 4 + count * 28;
        let crc = directory_crc(&bytes[base..end]).unwrap();
        bytes[end..end + 4].copy_from_slice(&crc.to_le_bytes());
    }
}

pub struct Workspace {
    pub temporary: tempfile::TempDir,
    pub root: std::path::PathBuf,
    pub bytes: Vec<u8>,
}

impl Workspace {
    pub fn new() -> Self {
        Self::with_pages(1)
    }
    pub fn with_pages(count: usize) -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let bytes = fixture_with_pages(count);
        let input = temporary.path().join("input.HMI");
        std::fs::write(&input, &bytes).unwrap();
        let root = temporary.path().join("project");
        hmi_compiler::workspace::export(&input, &root, false).unwrap();
        std::fs::remove_file(input).unwrap();
        Self {
            temporary,
            root,
            bytes,
        }
    }
    pub fn page_path(&self) -> std::path::PathBuf {
        self.root.join("pages/p000_page0/page.toml")
    }
    pub fn read_page(&self) -> hmi_compiler::workspace::PageFile {
        toml::from_str(&std::fs::read_to_string(self.page_path()).unwrap()).unwrap()
    }
    pub fn write_page(&self, page: &hmi_compiler::workspace::PageFile) {
        std::fs::write(
            self.page_path(),
            hmi_compiler::workspace::page_to_toml(page).unwrap(),
        )
        .unwrap();
    }
}
