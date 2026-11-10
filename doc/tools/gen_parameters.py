import re,sys
src=open('core/src/params.rs').read()
ui=open('app/src/schema.ts').read()
labels={}
hints={}
for m in re.finditer(r"p\('(\w+)',\s*'(\w+)',\s*'((?:[^'\\]|\\.)*)',\s*([-\d.e]+),\s*([-\d.e]+),\s*[-\d.e]+(?:,\s*'((?:[^'\\]|\\.)*)')?", ui):
    labels[(m.group(1),m.group(2))]=m.group(3).replace("\\'","'")+f" ({m.group(4)}–{m.group(5)})"
    if m.group(6): hints[(m.group(1),m.group(2))]=m.group(6).replace("\\'","'")
keys={'PlanetParams':'planet','SketchParams':'sketch','PlateParams':'plates','TectonicParams':'tectonics','ClimateParams':'climate','HydroParams':'hydrology','BiomeParams':'biomes','HabitabilityParams':'habitability','StateParams':'states','ProvinceParams':'provinces','CultureParams':'cultures','NationParams':'nations'}
titles={'planet':'Planet (step 1)','sketch':'Continent sketch (step 2)','plates':'Plates (step 3)','tectonics':'Tectonic relief (step 4)','climate':'Climate (step 5)','hydrology':'Hydrology and erosion (step 6)','biomes':'Biomes (step 7)','habitability':'Habitability and barriers (step 8)','states':'States (step 9)','provinces':'Provinces (step 10)','cultures':'Cultures (step 11)','nations':'Nations and history (step 12)'}
out=["# Parameter reference","","Every parameter of the twelve steps, with its default and meaning. This file is generated from `core/src/params.rs` (doc comments and `Default` impls) and the UI labels in `app/src/schema.ts`; regenerate it when parameters change. In `world.json` the parameters live under `params.<section>.<name>`; a missing value takes its default.",""]
for st,key in keys.items():
    m=re.search(r"pub struct "+st+r" \{(.*?)\n\}",src,re.S)
    body=m.group(1)
    d=re.search(r"impl Default for "+st+r" \{.*?"+st+r" \{(.*?)\}\s*\n\s*\}\s*\n\}",src,re.S).group(1)
    defs={}
    depth=0; cur=''
    parts=[]
    for ch in d:
        if ch in '([': depth+=1
        if ch in ')]': depth-=1
        if ch==',' and depth==0: parts.append(cur); cur=''
        else: cur+=ch
    parts.append(cur)
    for part in parts:
        if ':' in part:
            k,v=part.split(':',1); k=k.strip().split()[-1] if k.strip() else ''
            defs[k]=' '.join(v.split())
    rows=[]; doc=[]; lastdoc=''; lastname=''
    for line in body.split('\n'):
        t=line.strip()
        if t.startswith('///'):
            doc.append(t[3:].strip()); continue
        f=re.match(r"pub (\w+):\s*(.+?),",t)
        if not f:
            doc=[]; continue
        name,ty=f.group(1),f.group(2)
        if doc:
            text=' '.join(doc); lastdoc=text
        elif lastname and set(name.split('_')) & set(lastname.split('_')):
            text=lastdoc  # a doc comment shared by a group of related fields
        else:
            text=hints.get((key,name),''); lastdoc=''
        doc=[]; lastname=name
        rows.append((name,ty,defs.get(name,''),labels.get((key,name),''),text))
    out+=["## "+titles[key],"","| Name | UI label | Default | Meaning |","| --- | --- | --- | --- |"]
    for name,ty,dv,lab,text in rows:
        dv=dv.replace('|','\\|')
        out.append(f"| `{name}` | {lab or '—'} | `{dv}` | {text.replace('|','\\|') or '—'} |")
    out.append("")
open('doc/parameters.md','w').write('\n'.join(out))
print(len(out))
