# litematic2hnproj

把 **Java 版 Litematica 的 `.litematic`** 转成 **Starling 手机端投影（`schematic` 模组）能直接读的 `.hnproj`**。

纯 Rust，只有一个依赖（`flate2`，解 gzip），自己解析 NBT，不依赖任何 Minecraft 库。源码就一个 `main.rs`，方便照着改。

## 编译

```bash
cargo build --release
# 产物：target/release/litematic2hnproj
```

Linux / macOS / Windows 都一样，`rustup` 装好就能编，不需要 JDK 也不需要 Minecraft 本体。

## 用法

```bash
# 看里面有几个区域、尺寸、方块数（不动文件）
litematic2hnproj --list 刷怪塔.litematic

# 转（默认转体积最大的那个区域）
litematic2hnproj 刷怪塔.litematic -o 刷怪塔.hnproj

# 多个区域合成一个蓝图（各自按相对位置拼起来）
litematic2hnproj --all 刷怪塔.litematic -o 刷怪塔.hnproj

# 指定蓝图里的名字（不填就用文件名）
litematic2hnproj 刷怪塔.litematic -n "沼泽刷怪塔" -o tower.hnproj
```

| 参数 | 说明 |
|---|---|
| `-o, --out <路径>` | 输出文件，默认 `<源文件名>.hnproj` |
| `-n, --name <名字>` | 蓝图显示名，支持中文/空格/括号；默认取源文件名 |
| `--all` | 合并全部区域；不加则只转体积最大的那个 |
| `--list` | 只打印区域清单与统计，不写文件 |
| `-h, --help` | 帮助 |

没有「只转第 N 个区域」的开关：单区域文件直接转，多区域文件要么取最大的那个、要么用 `--all` 全拼起来（相对位置保持原样）。

## 输出格式（`hnproj 2`）

```
hnproj 2
name=双TNT处死沼泽刷怪塔v1.2(效率22800)
size=51 129 40
p 0 minecraft:glass
p 1 minecraft:smooth_stone
p 2 minecraft:repeater[delay=4,facing=east,locked=true,powered=true]
...
0 0 0 1
0 1 0 1
...
```

- `p 序号 完整方块状态`：调色板，同一种状态只写一次，**属性一个不丢**（朝向、档位、水位、shape、half、powered 全带）
- `x y z 序号`：非空气方块，坐标相对蓝图原点
- 名字支持中文、空格、括号

纯文本格式，行序无关，解析器按前缀区分；要手工改也改得动。

## 怎么放进游戏

1. 把 `.hnproj` 丢进 Starling 游戏目录下的 `projections/` 文件夹。
2. 进游戏用 `/proj` 指令，Tab 补全里能看到蓝图名。
3. `/proj layer up` / `/proj layer down` 切层，单层高度是固定的、不跟着人走，拿木棍可以挪位置。

`.hnproj` 只描述“哪里该有什么方块”，不含任何游戏逻辑；放置还是你自己放，投影负责显示半透明鬼影和缺块标记。

## 支持 / 不支持

| 支持 | 不支持 |
|---|---|
| `.litematic`（Litematica 蓝图，含多区域） | `.schem`（Sponge/WorldEdit 格式） |
| 任意方块状态与属性 | 实体、方块实体内部数据（箱子里的东西、告示牌文字等不转） |
| 1.20+ 新坐标格式与旧 IntArray 格式 | 结构文件 `.nbt`（原版 structure block 导出） |
| 负尺寸区域（自动归一化原点） | 超过投影上限的超大蓝图（手机端有单边/总块数限制，见 Starling 内提示） |

## 已验证的样例

仓库里带了一份真机验证过的小样本：`样本-双TNT处死沼泽刷怪塔.litematic`

| 文件 | 源 | 结果 |
|---|---|---|
| `双TNT处死沼泽刷怪塔v1.2(效率22800).litematic` | 51×129×40，DataVersion 4435 | 6,994 格非空气 / 118 种状态，与独立解析（Python nbtlib）**逐状态 0 差异** |

同目录下的 `示例-刷怪塔转换结果.hnproj`、`示例-红石样机.hnproj` 就是输出长什么样，可以直接丢进 `projections/` 试。

## 两个坑（已经踩过并修好）

1. **位数组是「连续打包」不是「补位」**。Litematica 的 `LitematicaBitArray`：第 i 个值从第 `i*bits` 个 bit 开始，允许跨 64 位借下一格的高位。按旧式「每个 long 塞满整数个值、尾巴留空」读会整体错位，方块会串行、丢格。判断方法：数组长度应等于 `ceil(格数 * bits / 64)`。
2. **1.20 之后坐标是 `{x,y,z}` 复合标签**，不是旧的三项 IntArray；负尺寸表示区域往负方向长，要先归一化再算原点。

## 注意

- `.hnproj` 只是**蓝图**，本身不含游戏逻辑；Starling 里放到游戏目录的 `projections/`，`/proj` 就能看到。
- 水、岩浆、气泡柱这类方块在原版没有方块模型（是流体渲染的），投影里会用半透明玻璃色块顶上，不会凭空消失（需要内核支持）。
- 投影显示会把蓝图里**所有**方块都画出来，包括被完全包在内部的（平滑石、音符盒这类藏在机器里的不会漏）。如果某个位置空着，先核对 `.litematic` 原文件里那儿到底有没有方块。

## 开发

单文件实现，结构很直白：`read_gzip` → `parse_nbt`（自写 tag 解析）→ 取 `Regions` 里各区域的 `BlockStates` / `Palette` / `Size` / `Position` → 解位数组 → 写文本。
要加 `.schem` 支持，只需再加一个 `parser` 分支；要加批量/Action 自动转换，把 CLI 包一层就行。

## 免责

仅用于自己存档的蓝图格式转换。Litematica / Litematica 的作者与 Starling 的作者均与本项目无关；请只转换你自己有权使用的蓝图。
