//! 把 Java 版 Litematica 的 `.litematic` 转成 Starling 手机端投影用的 `.hnproj`（格式 hnproj 2）。
//!
//! `.hnproj 2` 的关键点：每种方块状态完整存一行（比较器的 delay/facing/mode、中继器、铁轨
//! 的 shape/powered、活塞朝向、楼梯 half/shape、半砖 type 一个不丢），方块行只写
//! `x y z 调色板序号`。Java 版 litematic 的调色板本来就是 `minecraft:oak_stairs[facing=east,...]`
//! 这种完整状态文本，所以这里原样搬过去即可（属性按名字排序，和客户端读的一致）。
use flate2::read::MultiGzDecoder;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut name: Option<String> = None;
    let mut merge_all = false;
    let mut list_only = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-o" | "--out" => output = args.next().map(PathBuf::from),
            "-n" | "--name" => name = args.next(),
            "--all" => merge_all = true,
            "--list" => list_only = true,
            "-h" | "--help" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            other if other.starts_with('-') => {
                eprintln!("不认得这个参数：{other}");
                print_help();
                return ExitCode::FAILURE;
            }
            other => input = Some(PathBuf::from(other)),
        }
    }
    let Some(input) = input else {
        print_help();
        return ExitCode::FAILURE;
    };
    match run(&input, output, name, merge_all, list_only) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("失败：{e}");
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!(
        "litematic2hnproj —— Java 版 .litematic → Starling 手机端 .hnproj\n\n\
用法：\n\
  litematic2hnproj 机器.litematic                   转体积最大的那个区域（默认）\n\
  litematic2hnproj 机器.litematic --all             把所有区域按相对位置合成一个\n\
  litematic2hnproj 机器.litematic -o 我的机器.hnproj 指定输出文件\n\
  litematic2hnproj 机器.litematic -n 我的机器        指定蓝图名字（默认用文件名）\n\
  litematic2hnproj 机器.litematic --list            只列区域，不转换\n\n\
转好的 .hnproj 丢进 Starling 的 projections/ 目录，游戏里 /proj list 就能看到。"
    );
}

fn run(
    input: &Path,
    output: Option<PathBuf>,
    name: Option<String>,
    merge_all: bool,
    list_only: bool,
) -> Result<(), String> {
    let raw = std::fs::read(input).map_err(|e| format!("读不了 {}：{e}", input.display()))?;
    let nbt = inflate(&raw)?;
    let root = parse_nbt(&nbt)?;
    let lite = read_litematic(&root)?;
    if lite.regions.is_empty() {
        return Err("这个文件里一个区域都没有（是不是空的框选？）".to_owned());
    }
    if list_only {
        println!("共 {} 个区域：", lite.regions.len());
        for r in &lite.regions {
            println!(
                "  {}  位置 ({}, {}, {})  尺寸 {}×{}×{}  {} 格非空气",
                r.name,
                r.pos[0],
                r.pos[1],
                r.pos[2],
                r.size[0],
                r.size[1],
                r.size[2],
                r.blocks.len()
            );
        }
        return Ok(());
    }
    let picked: Vec<&Region> = if merge_all {
        lite.regions.iter().collect()
    } else {
        let mut best = &lite.regions[0];
        for r in &lite.regions {
            let v = vol(r.size);
            if v > vol(best.size) {
                best = r;
            }
        }
        vec![best]
    };
    if picked.len() > 1 {
        eprintln!(
            "注意：合并 {} 个区域（--all）。单个文件里有多个区域时，默认只转最大的那个。",
            picked.len()
        );
    }

    // 合并后的整体包围盒（区域坐标是相对同一个原点的）。
    let mut lo = [i32::MAX; 3];
    let mut hi = [i32::MIN; 3];
    for r in &picked {
        for axis in 0..3 {
            lo[axis] = lo[axis].min(r.pos[axis]);
            hi[axis] = hi[axis].max(r.pos[axis] + r.size[axis] - 1);
        }
    }
    let size = [hi[0] - lo[0] + 1, hi[1] - lo[1] + 1, hi[2] - lo[2] + 1];

    let mut palette: Vec<String> = Vec::new();
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    let mut cells: Vec<([i32; 3], usize)> = Vec::new();
    for r in &picked {
        for (rel, state) in &r.blocks {
            let world = [
                r.pos[0] + rel[0] - lo[0],
                r.pos[1] + rel[1] - lo[1],
                r.pos[2] + rel[2] - lo[2],
            ];
            let idx = match index.get(state) {
                Some(&i) => i,
                None => {
                    let i = palette.len();
                    palette.push(state.clone());
                    index.insert(state.clone(), i);
                    i
                }
            };
            cells.push((world, idx));
        }
    }
    if cells.is_empty() {
        return Err("这个文件里没有方块（可能全是空气）".to_owned());
    }
    cells.sort_by_key(|(rel, _)| (rel[0], rel[1], rel[2]));

    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("projection")
        .to_owned();
    let title = match name {
        Some(n) if !n.trim().is_empty() => n.trim().to_owned(),
        _ => lite
            .title
            .clone()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or(stem),
    };
    let out_path = output.unwrap_or_else(|| input.with_extension("hnproj"));

    let mut body = String::with_capacity(64 + cells.len() * 12 + palette.len() * 80);
    let _ = writeln!(body, "hnproj 2");
    let _ = writeln!(body, "name={title}");
    let _ = writeln!(body, "size={} {} {}", size[0], size[1], size[2]);
    for (i, state) in palette.iter().enumerate() {
        let _ = writeln!(body, "p {i} {state}");
    }
    for (rel, idx) in &cells {
        let _ = writeln!(body, "{} {} {} {}", rel[0], rel[1], rel[2], idx);
    }
    std::fs::write(&out_path, body).map_err(|e| format!("写不了 {}：{e}", out_path.display()))?;

    println!(
        "转好了：{}\n蓝图「{}」  {}×{}×{}  {} 格方块  {} 种状态{}",
        out_path.display(),
        title,
        size[0],
        size[1],
        size[2],
        cells.len(),
        palette.len(),
        if picked.len() > 1 {
            format!("（合并了 {} 个区域）", picked.len())
        } else {
            String::new()
        }
    );
    Ok(())
}

fn vol(s: [i32; 3]) -> i64 {
    i64::from(s[0].max(0)) * i64::from(s[1].max(0)) * i64::from(s[2].max(0))
}

/// 文件头按理是 gzip；不是的话就当未压缩的 NBT 直接读。
fn inflate(raw: &[u8]) -> Result<Vec<u8>, String> {
    if raw.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        MultiGzDecoder::new(raw)
            .read_to_end(&mut out)
            .map_err(|e| format!("gzip 解不开：{e}"))?;
        Ok(out)
    } else {
        Ok(raw.to_vec())
    }
}

// ---------------- NBT ----------------

#[derive(Debug, Clone)]
enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    Str(String),
    List(Vec<Tag>),
    Compound(BTreeMap<String, Tag>),
    Ints(Vec<i32>),
    Longs(Vec<i64>),
    Bytes(Vec<u8>),
}

impl Tag {
    fn compound(&self) -> Result<&BTreeMap<String, Tag>, String> {
        match self {
            Tag::Compound(c) => Ok(c),
            _ => Err("这里该是 compound，实际不是".to_owned()),
        }
    }
    fn list(&self) -> Result<&[Tag], String> {
        match self {
            Tag::List(l) => Ok(l),
            _ => Err("这里该是 list，实际不是".to_owned()),
        }
    }
    fn longs(&self) -> Result<&[i64], String> {
        match self {
            Tag::Longs(v) => Ok(v),
            Tag::Long(l) => Ok(std::slice::from_ref(l)),
            _ => Err("这里该是 long 数组，实际不是".to_owned()),
        }
    }
    fn int(&self) -> Result<i32, String> {
        match self {
            Tag::Byte(v) => Ok(i32::from(*v)),
            Tag::Short(v) => Ok(i32::from(*v)),
            Tag::Int(v) => Ok(*v),
            Tag::Long(v) => Ok(*v as i32),
            _ => Err("这里该是整数，实际不是".to_owned()),
        }
    }
    fn string(&self) -> Result<&str, String> {
        match self {
            Tag::Str(s) => Ok(s),
            _ => Err("这里该是字符串，实际不是".to_owned()),
        }
    }
}

fn parse_nbt(bytes: &[u8]) -> Result<Tag, String> {
    let mut cur = 0usize;
    let kind = read_u8(bytes, &mut cur).ok_or("文件太短，不像 NBT")?;
    if kind != 10 {
        return Err("根标签不是 compound（不是标准 NBT）".to_owned());
    }
    let _root_name = read_str(bytes, &mut cur)?;
    read_payload(10, bytes, &mut cur)
}

fn read_payload(kind: u8, b: &[u8], cur: &mut usize) -> Result<Tag, String> {
    Ok(match kind {
        1 => Tag::Byte(read_u8(b, cur).ok_or("缺 byte")? as i8),
        2 => Tag::Short(read_u16(b, cur).ok_or("缺 short")? as i16),
        3 => Tag::Int(read_u32(b, cur).ok_or("缺 int")? as i32),
        4 => Tag::Long(read_u64(b, cur).ok_or("缺 long")? as i64),
        5 => Tag::Float(f32::from_bits(read_u32(b, cur).ok_or("缺 float")?)),
        6 => Tag::Double(f64::from_bits(read_u64(b, cur).ok_or("缺 double")?)),
        7 => {
            let n = read_u32(b, cur).ok_or("缺 byte 数组长度")? as usize;
            let mut v = Vec::with_capacity(n);
            for _ in 0..n {
                v.push(read_u8(b, cur).ok_or("byte 数组被截断")?);
            }
            Tag::Bytes(v)
        }
        8 => Tag::Str(read_str(b, cur)?),
        9 => {
            let inner = read_u8(b, cur).ok_or("缺 list 元素类型")?;
            let n = read_u32(b, cur).ok_or("缺 list 长度")? as usize;
            let mut v = Vec::with_capacity(n.min(1 << 20));
            for _ in 0..n {
                v.push(read_payload(inner, b, cur)?);
            }
            Tag::List(v)
        }
        10 => {
            let mut map = BTreeMap::new();
            loop {
                let t = read_u8(b, cur).ok_or("compound 被截断")?;
                if t == 0 {
                    break;
                }
                let name = read_str(b, cur)?;
                let value = read_payload(t, b, cur)?;
                map.insert(name, value);
            }
            Tag::Compound(map)
        }
        11 => {
            let n = read_u32(b, cur).ok_or("缺 int 数组长度")? as usize;
            let mut v = Vec::with_capacity(n.min(1 << 22));
            for _ in 0..n {
                v.push(read_u32(b, cur).ok_or("int 数组被截断")? as i32);
            }
            Tag::Ints(v)
        }
        12 => {
            let n = read_u32(b, cur).ok_or("缺 long 数组长度")? as usize;
            let mut v = Vec::with_capacity(n.min(1 << 22));
            for _ in 0..n {
                v.push(read_u64(b, cur).ok_or("long 数组被截断")? as i64);
            }
            Tag::Longs(v)
        }
        other => return Err(format!("不认得的 NBT 标签类型：{other}")),
    })
}

fn read_u8(b: &[u8], cur: &mut usize) -> Option<u8> {
    let v = *b.get(*cur)?;
    *cur += 1;
    Some(v)
}
fn read_u16(b: &[u8], cur: &mut usize) -> Option<u16> {
    Some(u16::from_be_bytes([read_u8(b, cur)?, read_u8(b, cur)?]))
}
fn read_u32(b: &[u8], cur: &mut usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        read_u8(b, cur)?,
        read_u8(b, cur)?,
        read_u8(b, cur)?,
        read_u8(b, cur)?,
    ]))
}
fn read_u64(b: &[u8], cur: &mut usize) -> Option<u64> {
    let hi = read_u32(b, cur)? as u64;
    let lo = read_u32(b, cur)? as u64;
    Some((hi << 32) | lo)
}
fn read_str(b: &[u8], cur: &mut usize) -> Result<String, String> {
    let n = read_u16(b, cur).ok_or("缺字符串长度")? as usize;
    let end = cur.checked_add(n).ok_or("字符串长度溢出")?;
    let slice = b.get(*cur..end).ok_or("字符串被截断")?;
    *cur = end;
    String::from_utf8(slice.to_vec()).map_err(|e| format!("字符串不是 UTF-8：{e}"))
}

// ---------------- litematic ----------------

struct Region {
    name: String,
    /// 归一化后的原点（最靠 -x -y -z 的那个角）。
    pos: [i32; 3],
    size: [i32; 3],
    /// 区域内部相对坐标 + 完整状态文本。
    blocks: Vec<([i32; 3], String)>,
}

struct Lite {
    title: Option<String>,
    regions: Vec<Region>,
}

fn read_litematic(root: &Tag) -> Result<Lite, String> {
    let c = root.compound()?;
    let mut title = None;
    if let Some(Tag::Compound(meta)) = c.get("Metadata") {
        if let Some(Tag::Str(n)) = meta.get("Name") {
            title = Some(n.clone());
        }
    }
    let mut regions = Vec::new();
    let Some(rs) = c.get("Regions") else {
        return Ok(Lite { title, regions });
    };
    for (name, region) in rs.compound()? {
        let r = region.compound()?;
        let pos = read_vec(r.get("Position"))?;
        let raw_size = read_vec(r.get("Size"))?;
        // Litematica 用负尺寸表示往负方向长：起点要挪到 size+1，尺寸取正。
        let mut origin = pos;
        let mut size = raw_size;
        for axis in 0..3 {
            if size[axis] < 0 {
                origin[axis] += size[axis] + 1;
                size[axis] = -size[axis];
            }
        }
        let palette = r
            .get("BlockStatePalette")
            .ok_or("区域里没有 BlockStatePalette")?
            .list()?;
        let states = palette_states(palette)?;
        let states_len = states.len();
        let mut blocks = Vec::new();
        if states_len > 1 {
            let longs = r.get("BlockStates").ok_or("区域里没有 BlockStates")?.longs()?;
            let bits = bits_for(states_len) as usize;
            let total = (size[0].max(0) as usize) * (size[1].max(0) as usize) * (size[2].max(0) as usize);
            let mask = (1u64 << bits) - 1;
            // Litematica 的 LitematicaBitArray 是“整条连续打包”，不补位：
            // 第 i 个值从第 i*bits 个 bit 开始，允许跨 64 位借下一格的高位。
            for y in 0..size[1].max(0) {
                for z in 0..size[2].max(0) {
                    for x in 0..size[0].max(0) {
                        let i = ((y as usize) * (size[2].max(0) as usize) + z as usize)
                            * (size[0].max(0) as usize)
                            + x as usize;
                        if i >= total {
                            break;
                        }
                        let start_offset = i * bits;
                        let start = start_offset >> 6;
                        let offset = start_offset & 63;
                        let Some(&raw) = longs.get(start) else {
                            continue;
                        };
                        // Java 版按“跨 64 位”拼：低位在前，不够就借下一格的高位。
                        let mut v = (raw as u64 >> offset) & mask;
                        if offset + bits > 64 {
                            if let Some(&next) = longs.get(start + 1) {
                                v |= (next as u64) << (64 - offset);
                                v &= mask;
                            }
                        }
                        let idx = v as usize;
                        if idx >= states_len {
                            continue;
                        }
                        let text = &states[idx];
                        if is_air(text) {
                            continue;
                        }
                        blocks.push(([x, y, z], text.clone()));
                    }
                }
            }
        }
        regions.push(Region {
            name: name.clone(),
            pos: origin,
            size,
            blocks,
        });
    }
    Ok(Lite { title, regions })
}

fn read_vec(tag: Option<&Tag>) -> Result<[i32; 3], String> {
    let Some(tag) = tag else {
        return Err("区域缺坐标".to_owned());
    };
    match tag {
        Tag::List(items) if items.len() >= 3 => Ok([
            items[0].int()?,
            items[1].int()?,
            items[2].int()?,
        ]),
        Tag::Ints(v) if v.len() >= 3 => Ok([v[0], v[1], v[2]]),
        // 1.20 之后（DataVersion 3xxx 起）vec3i 存成 {x,y,z} compound。
        Tag::Compound(c) => Ok([
            c.get("x").ok_or("vec3i 缺 x")?.int()?,
            c.get("y").ok_or("vec3i 缺 y")?.int()?,
            c.get("z").ok_or("vec3i 缺 z")?.int()?,
        ]),
        _ => Err("坐标/尺寸不是三项整数".to_owned()),
    }
}

fn bits_for(palette_len: usize) -> u32 {
    let mut bits = 2u32;
    while (1usize << bits) < palette_len {
        bits += 1;
    }
    bits
}

/// litematic 调色板每一项 → `minecraft:stone` / `minecraft:oak_stairs[facing=east,shape=straight]`。
fn palette_states(palette: &[Tag]) -> Result<Vec<String>, String> {
    let mut out = Vec::with_capacity(palette.len());
    for entry in palette {
        let c = entry.compound()?;
        let name = c
            .get("Name")
            .ok_or("调色板里有一项没有 Name")?
            .string()?
            .to_owned();
        let name = if name.contains(':') {
            name
        } else {
            format!("minecraft:{name}")
        };
        let mut parts = Vec::new();
        if let Some(Tag::Compound(props)) = c.get("Properties") {
            for (k, v) in props {
                if let Tag::Str(v) = v {
                    parts.push(format!("{k}={v}"));
                }
            }
            parts.sort();
        }
        out.push(if parts.is_empty() {
            name
        } else {
            format!("{name}[{}]", parts.join(","))
        });
    }
    Ok(out)
}

fn is_air(state: &str) -> bool {
    state == "minecraft:air"
        || state == "minecraft:cave_air"
        || state == "minecraft:void_air"
        || state == "minecraft:structure_void"
}
