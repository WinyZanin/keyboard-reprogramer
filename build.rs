//! Embute no executável o ícone (assets/icon.ico) e as informações de versão.
//!
//! Gera direto um objeto COFF com a seção `.rsrc`, no mesmo formato que `windres`/`cvtres`
//! produzem, e o passa para o linker. Assim o build não depende de compilador de
//! recursos nenhum: basta o `cargo`.

use std::env;
use std::fs;
use std::path::PathBuf;

const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const RT_VERSION: u16 = 16;
/// Português (Brasil).
const LANG: u32 = 0x0416;

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let (machine, reloc_type) = match env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "x86_64" => (0x8664u16, 3u16), // IMAGE_REL_AMD64_ADDR32NB
        "x86" => (0x014c, 7),          // IMAGE_REL_I386_DIR32NB
        "aarch64" => (0xAA64, 2),      // IMAGE_REL_ARM64_ADDR32NB
        other => {
            println!("cargo:warning=arquitetura {other} sem suporte a recursos; exe sem ícone");
            return;
        }
    };

    let ico = fs::read("assets/icon.ico").expect("assets/icon.ico não encontrado");
    let mut resources = icon_resources(&ico);
    resources.push((RT_VERSION, 1, version_info()));

    let obj = coff_object(machine, reloc_type, &resources);
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("resources.o");
    fs::write(&out, obj).unwrap();
    // Passado direto ao linker: o objeto entra no exe com a seção .rsrc.
    println!("cargo:rustc-link-arg-bins={}", out.display());
}

fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(b[i..i + 4].try_into().unwrap())
}

/// Converte um .ico em recursos: cada imagem vira um RT_ICON (IDs 1..n) e o
/// diretório vira o RT_GROUP_ICON de ID 1, que é o que o Explorer e o LoadImage usam.
fn icon_resources(ico: &[u8]) -> Vec<(u16, u16, Vec<u8>)> {
    assert!(u16_at(ico, 2) == 1, "assets/icon.ico não é um ícone válido");
    let count = u16_at(ico, 4) as usize;
    let mut resources = Vec::new();
    let mut group = Vec::new();
    group.extend_from_slice(&[0, 0, 1, 0]); // reserved, type = ícone
    group.extend_from_slice(&(count as u16).to_le_bytes());
    for i in 0..count {
        let e = 6 + i * 16;
        let size = u32_at(ico, e + 8) as usize;
        let offset = u32_at(ico, e + 12) as usize;
        let id = (i + 1) as u16;
        resources.push((RT_ICON, id, ico[offset..offset + size].to_vec()));
        // GRPICONDIRENTRY: os mesmos 12 primeiros bytes da entrada do .ico + ID do recurso.
        group.extend_from_slice(&ico[e..e + 12]);
        group.extend_from_slice(&id.to_le_bytes());
    }
    resources.push((RT_GROUP_ICON, 1, group));
    resources
}

fn utf16z(s: &str) -> Vec<u8> {
    s.encode_utf16().chain(Some(0)).flat_map(u16::to_le_bytes).collect()
}

fn pad4(buf: &mut Vec<u8>) {
    while !buf.len().is_multiple_of(4) {
        buf.push(0);
    }
}

/// Bloco genérico da estrutura VS_VERSIONINFO: wLength, wValueLength, wType, szKey, Value, Children.
fn ver_block(key: &str, value: &[u8], value_len: u16, text: bool, children: &[Vec<u8>]) -> Vec<u8> {
    let mut b = vec![0u8; 6];
    b[2..4].copy_from_slice(&value_len.to_le_bytes());
    b[4..6].copy_from_slice(&(text as u16).to_le_bytes());
    b.extend(utf16z(key));
    pad4(&mut b);
    b.extend_from_slice(value);
    for child in children {
        pad4(&mut b);
        b.extend_from_slice(child);
    }
    let len = b.len() as u16;
    b[0..2].copy_from_slice(&len.to_le_bytes());
    b
}

fn ver_string(key: &str, value: &str) -> Vec<u8> {
    let v = utf16z(value);
    ver_block(key, &v, (v.len() / 2) as u16, true, &[])
}

/// Recurso RT_VERSION (aba "Detalhes" das propriedades do exe), com a versão do Cargo.toml.
fn version_info() -> Vec<u8> {
    let part = |name: &str| env::var(name).unwrap().parse::<u32>().unwrap_or(0);
    let (major, minor, patch) = (
        part("CARGO_PKG_VERSION_MAJOR"),
        part("CARGO_PKG_VERSION_MINOR"),
        part("CARGO_PKG_VERSION_PATCH"),
    );
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let ms = (major << 16) | minor;
    let ls = patch << 16;

    let mut fixed = Vec::new();
    for v in [
        0xFEEF04BDu32, // assinatura
        0x0001_0000,   // versão da estrutura
        ms, ls,        // FileVersion
        ms, ls,        // ProductVersion
        0x3F, 0,       // FileFlagsMask, FileFlags
        0x0004_0004,   // VOS_NT_WINDOWS32
        1, 0,          // VFT_APP, subtipo
        0, 0,          // data
    ] {
        fixed.extend_from_slice(&v.to_le_bytes());
    }

    let name = "Keyboard Reprogramer";
    let exe = format!("{}.exe", env::var("CARGO_PKG_NAME").unwrap());
    let strings = [
        ver_string("FileDescription", name),
        ver_string("ProductName", name),
        ver_string("FileVersion", &version),
        ver_string("ProductVersion", &version),
        ver_string("OriginalFilename", &exe),
        ver_string("InternalName", &exe),
    ];
    let table = ver_block(&format!("{LANG:04X}04B0"), &[], 0, true, &strings);
    let string_info = ver_block("StringFileInfo", &[], 0, true, &[table]);
    let translation = [(LANG as u16).to_le_bytes(), 1200u16.to_le_bytes()].concat(); // UTF-16
    let var = ver_block("Translation", &translation, 4, false, &[]);
    let var_info = ver_block("VarFileInfo", &[], 0, true, &[var]);
    ver_block("VS_VERSION_INFO", &fixed, fixed.len() as u16, false, &[string_info, var_info])
}

/// Monta um objeto COFF com uma seção `.rsrc` contendo a árvore de recursos
/// (tipo -> ID -> idioma -> dados). Os ponteiros para os dados são RVAs, resolvidos
/// pelo linker através de relocações contra o símbolo da seção.
fn coff_object(machine: u16, reloc_type: u16, resources: &[(u16, u16, Vec<u8>)]) -> Vec<u8> {
    let mut res: Vec<&(u16, u16, Vec<u8>)> = resources.iter().collect();
    res.sort_by_key(|r| (r.0, r.1)); // as entradas de cada diretório precisam estar ordenadas
    let mut types: Vec<u16> = res.iter().map(|r| r.0).collect();
    types.dedup();

    // Layout: raiz, diretórios de tipo, diretórios de idioma (1 por recurso), entradas de dados, dados.
    let dir_size = |entries: usize| 16 + 8 * entries;
    let root_size = dir_size(types.len());
    let mut type_dir_off = Vec::new();
    let mut off = root_size;
    for t in &types {
        type_dir_off.push(off);
        off += dir_size(res.iter().filter(|r| r.0 == *t).count());
    }
    let lang_dir_off: Vec<usize> = (0..res.len()).map(|i| off + i * dir_size(1)).collect();
    off += res.len() * dir_size(1);
    let data_entry_off: Vec<usize> = (0..res.len()).map(|i| off + i * 16).collect();
    off += res.len() * 16;
    let mut data_off = Vec::new();
    for r in &res {
        off = (off + 7) & !7;
        data_off.push(off);
        off += r.2.len();
    }

    let mut sec = vec![0u8; off];
    let put16 = |s: &mut Vec<u8>, at: usize, v: u16| s[at..at + 2].copy_from_slice(&v.to_le_bytes());
    let put32 = |s: &mut Vec<u8>, at: usize, v: u32| s[at..at + 4].copy_from_slice(&v.to_le_bytes());
    const SUBDIR: u32 = 0x8000_0000;

    // IMAGE_RESOURCE_DIRECTORY só com entradas por ID; as entradas vêm logo depois.
    let write_dir = |s: &mut Vec<u8>, at: usize, entries: &[(u32, u32)]| {
        put16(s, at + 14, entries.len() as u16);
        for (i, (id, target)) in entries.iter().enumerate() {
            put32(s, at + 16 + i * 8, *id);
            put32(s, at + 20 + i * 8, *target);
        }
    };

    let root: Vec<(u32, u32)> =
        types.iter().zip(&type_dir_off).map(|(t, o)| (*t as u32, SUBDIR | *o as u32)).collect();
    write_dir(&mut sec, 0, &root);
    for (t, at) in types.iter().zip(&type_dir_off) {
        let names: Vec<(u32, u32)> = res
            .iter()
            .enumerate()
            .filter(|(_, r)| r.0 == *t)
            .map(|(i, r)| (r.1 as u32, SUBDIR | lang_dir_off[i] as u32))
            .collect();
        write_dir(&mut sec, *at, &names);
    }
    let mut relocs = Vec::new();
    for (i, r) in res.iter().enumerate() {
        write_dir(&mut sec, lang_dir_off[i], &[(LANG, data_entry_off[i] as u32)]);
        // IMAGE_RESOURCE_DATA_ENTRY: RVA (offset na seção + relocação), tamanho, codepage.
        put32(&mut sec, data_entry_off[i], data_off[i] as u32);
        put32(&mut sec, data_entry_off[i] + 4, r.2.len() as u32);
        sec[data_off[i]..data_off[i] + r.2.len()].copy_from_slice(&r.2);
        relocs.push(data_entry_off[i] as u32);
    }

    // Objeto COFF: cabeçalho, 1 seção, dados, relocações, 1 símbolo, tabela de strings vazia.
    let raw_ptr = 20 + 40;
    let reloc_ptr = raw_ptr + sec.len();
    let sym_ptr = reloc_ptr + relocs.len() * 10;
    let mut o = Vec::new();
    o.extend_from_slice(&machine.to_le_bytes());
    o.extend_from_slice(&1u16.to_le_bytes()); // seções
    o.extend_from_slice(&0u32.to_le_bytes()); // timestamp
    o.extend_from_slice(&(sym_ptr as u32).to_le_bytes());
    o.extend_from_slice(&1u32.to_le_bytes()); // símbolos
    o.extend_from_slice(&0u16.to_le_bytes()); // optional header
    o.extend_from_slice(&0u16.to_le_bytes()); // characteristics

    o.extend_from_slice(b".rsrc\0\0\0");
    o.extend_from_slice(&0u32.to_le_bytes()); // VirtualSize
    o.extend_from_slice(&0u32.to_le_bytes()); // VirtualAddress
    o.extend_from_slice(&(sec.len() as u32).to_le_bytes());
    o.extend_from_slice(&(raw_ptr as u32).to_le_bytes());
    o.extend_from_slice(&(reloc_ptr as u32).to_le_bytes());
    o.extend_from_slice(&0u32.to_le_bytes()); // linenumbers
    o.extend_from_slice(&(relocs.len() as u16).to_le_bytes());
    o.extend_from_slice(&0u16.to_le_bytes());
    // INITIALIZED_DATA | ALIGN_4BYTES | MEM_READ
    o.extend_from_slice(&0x4030_0040u32.to_le_bytes());

    o.extend_from_slice(&sec);
    for r in relocs {
        o.extend_from_slice(&r.to_le_bytes());
        o.extend_from_slice(&0u32.to_le_bytes()); // símbolo 0 (.rsrc)
        o.extend_from_slice(&reloc_type.to_le_bytes());
    }
    o.extend_from_slice(b".rsrc\0\0\0");
    o.extend_from_slice(&0u32.to_le_bytes()); // valor
    o.extend_from_slice(&1i16.to_le_bytes()); // seção 1
    o.extend_from_slice(&0u16.to_le_bytes()); // tipo
    o.push(3); // IMAGE_SYM_CLASS_STATIC
    o.push(0); // aux
    o.extend_from_slice(&4u32.to_le_bytes()); // tabela de strings vazia
    o
}
