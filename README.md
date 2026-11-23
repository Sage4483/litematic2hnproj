# litematic2hnproj

把 **Java 版 Litematica 的 `.litematic`** 转成 **Starling 手机端投影（`schematic` 模组）能直接读的 `.hnproj`**。

纯 Rust，只有一个依赖（`flate2`，解 gzip），自己解析 NBT，不依赖任何 Minecraft 库。

## 编译

```bash
cargo build --release
# 产物：target/release/litematic2hnproj
```

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

## 已验证的样例

| 文件 | 源 | 结果 |
|---|---|---|
| `双TNT处死沼泽刷怪塔v1.2(效率22800).litematic` | 51×129×40，DataVersion 4435 | 6,994 格非空气 / 118 种状态，与独立解析（Python nbtlib）**逐状态 0 差异** |

## 两个坑（已经踩过并修好）

1. **位数组是「连续打包」不是「补位」**。Litematica 的 `LitematicaBitArray`：第 i 个值从第 `i*bits` 个 bit 开始，允许跨 64 位借下一格的高位。按旧式「每个 long 塞满整数个值、尾巴留空」读会整体错位，方块会串行、丢格。判断方法：数组长度应等于 `ceil(格数 * bits / 64)`。
2. **1.20 之后坐标是 `{x,y,z}` 复合标签**，不是旧的三项 IntArray；负尺寸表示区域往负方向长，要先归一化再算原点。

## 注意

- `.hnproj` 只是**蓝图**，本身不含游戏逻辑；Starling 里放到游戏目录的 `projections/`，`/proj list` 就能看到。
- 水、岩浆、气泡柱这类方块在原版没有方块模型（是流体渲染的），投影里会用半透明玻璃色块顶上，不会凭空消失（需要内核支持）。
