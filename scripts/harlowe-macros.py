# Writes crates/check/src/harlowe/table.rs from the macro reference built into the bundled Harlowe's format.js, so the
# checker knows exactly the macros of the Harlowe twee-tools ships. Run it after updating crates/build/formats/harlowe-3;
# the check crate's tests fail until it is.
#
#   python3 scripts/harlowe-macros.py
import json, os, re

root = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
source = open(os.path.join(root, 'crates/build/formats/harlowe-3/format.js'), encoding='utf-8').read()
version = re.search(r'"version"\s*:\s*"([^"]+)"', source).group(1)
entries = re.findall(r'name:"([^"]+)",sig:"([^"]*)",returnType:"([^"]*)",aka:\[([^\]]*)\].*?category:"([^"]*)"', source)
seen = set()
rows = []
for name, sig, returns, aka, category in entries:
    if name in seen:
        continue
    seen.add(name)
    akas = re.findall(r'"([^"]+)"', aka)
    rows.append((name, akas, sig.strip(), returns, category))
rows.sort()
lines = [
    f'//! Harlowe {version}\'s macros, from the reference in its format.js. Written by scripts/harlowe-macros.py: don\'t edit.',
    '',
    'use super::MacroDef;',
    '',
    f'pub(crate) const VERSION: &str = "{version}";',
    '',
    'pub(crate) const MACROS: &[MacroDef] = &[',
]
for name, akas, sig, returns, category in rows:
    aka = ', '.join(json.dumps(a) for a in akas)
    lines.append(f'    MacroDef {{ name: {json.dumps(name)}, aka: &[{aka}], sig: {json.dumps(sig)}, returns: {json.dumps(returns)}, category: {json.dumps(category)} }},')
lines.append('];')
open(os.path.join(root, 'crates/check/src/harlowe/table.rs'), 'w').write('\n'.join(lines) + '\n')
print(f'{len(rows)} macros from Harlowe {version}')
