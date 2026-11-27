"""Build an Earth heightmap for the Earth check (core/examples/earth_check.rs).

  python3 doc/tools/make_earth_heightmap.py <topology.png> <water-mask.png> <out.png>

Inputs: an equirectangular grey land-height image (NASA Blue Marble
"topography"/bump map: 0 = sea level, 255 = about 6,400 m) and a water mask
of the same projection (white = water). Both come, for example, with the
`three-globe` npm package (example/img/earth-topology.png, earth-water.png).
Output: a 16-bit PNG in the app's heightmap16 encoding
(metres = v / 65535 * 20000 - 11000).

The sources have no bathymetry, so the sea floor is synthetic: it deepens
with distance from land to about -4,000 m. The climate step only needs to
know where the sea is. Needs numpy and Pillow.
"""
import sys
import numpy as np
from PIL import Image
topo_p, water_p, out_p = sys.argv[1:4]
topo = np.asarray(Image.open(topo_p).convert('L')).astype(np.float64)
H, W = topo.shape
water = np.asarray(Image.open(water_p).convert('L').resize((W, H), Image.NEAREST)) > 128
land_m = np.maximum(topo * (6400.0 / 255.0), 2.0)
# Distance to land in km, by repeated 8-neighbour dilation (wraps in longitude).
lat = 90.0 - (np.arange(H) + 0.5) * 180.0 / H
px_km = 180.0 / H * 111.2
dist = np.where(~water, 0.0, np.inf)
reached = ~water
for k in range(1, 80):
    grown = reached.copy()
    for dy in (-1, 0, 1):
        for dx in (-1, 0, 1):
            if dy or dx:
                sh = np.roll(reached, dx, axis=1)
                sh = np.roll(sh, dy, axis=0) if dy else sh
                if dy == 1: sh[0, :] = False
                if dy == -1: sh[-1, :] = False
                grown |= sh
    new = grown & ~reached
    dist[new] = k * px_km
    reached = grown
dist[np.isinf(dist)] = 80 * px_km
depth = -(80.0 + 3920.0 * (1.0 - np.exp(-dist / 250.0)))
m = np.where(water, depth, land_m)
v = np.clip(np.round((m + 11000.0) / 20000.0 * 65535.0), 0, 65535).astype(np.uint16)
Image.fromarray(v).save(out_p)  # uint16 -> a 16-bit grey PNG
print('size', W, H, 'land share', round(float((~water).mean()), 3), 'max m', round(float(m.max())), 'min m', round(float(m.min())))
