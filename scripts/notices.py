# Writes THIRD-PARTY-NOTICES.md: the licences of everything twee-tools' binary includes, the built-in story formats
# and every crate it's built from, with each one's licence text as the crate ships it. The release puts it in every
# archive.
#
#   python3 scripts/notices.py [--target <triple>] > THIRD-PARTY-NOTICES.md
import glob, json, os, subprocess, sys

root = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
args = ['cargo', 'metadata', '--format-version', '1', '--locked']
if '--target' in sys.argv:
    args += ['--filter-platform', sys.argv[sys.argv.index('--target') + 1]]
meta = json.loads(subprocess.run(args, cwd=root, capture_output=True, encoding='utf-8', check=True).stdout)

ours = set(meta['workspace_members'])
# Only what the twee-tools binary depends on, not tests or build tools: follow normal dependencies from the CLI crate.
nodes = {n['id']: n for n in meta['resolve']['nodes']}
cli = next(p['id'] for p in meta['packages'] if p['name'] == 'twee-tools')
used, todo = set(), [cli]
while todo:
    node = nodes[todo.pop()]
    for dep in node['deps']:
        if any(k['kind'] in (None, 'build') for k in dep['dep_kinds']) and dep['pkg'] not in used:
            used.add(dep['pkg'])
            todo.append(dep['pkg'])
missing = set()
MIT = """Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the Software without restriction, including without limitation the
rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit
persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the
Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE
WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE."""
ZLIB = """This software is provided 'as-is', without any express or implied warranty. In no event will the authors be held
liable for any damages arising from the use of this software.

Permission is granted to anyone to use this software for any purpose, including commercial applications, and to alter
it and redistribute it freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim that you wrote the original software. If
   you use this software in a product, an acknowledgment in the product documentation would be appreciated but is not
   required.
2. Altered source versions must be plainly marked as such, and must not be misrepresented as being the original
   software.
3. This notice may not be removed or altered from any source distribution."""
packages = sorted((p for p in meta['packages'] if p['id'] in used and p['id'] not in ours), key=lambda p: (p['name'], p['version']))

out = ['# Third-party notices', '',
       'twee-tools is licensed under MIT or Apache-2.0. It includes the work of others, under their own licences, below.', '']
out += ['## Story formats', '', 'Built into twee-tools, and into the pages it builds.', '']
for licence in sorted(glob.glob(os.path.join(root, 'crates/build/formats/*/LICENSE'))):
    name = os.path.basename(os.path.dirname(licence))
    out += [f'### {name}', '', '```', open(licence, encoding='utf-8').read().strip(), '```', '']
out += ['## Rust crates', '', f'{len(packages)} crates, each under the licence it declares. Their source is on crates.io: '
        'https://crates.io/crates/<name>/<version>.', '']
for p in packages:
    folder = os.path.dirname(p['manifest_path'])
    texts = sorted(f for f in os.listdir(folder) if f.upper().startswith(('LICENSE', 'LICENCE', 'COPYING', 'NOTICE', 'UNLICENSE')))
    out += [f"### {p['name']} {p['version']}", '', f"Licence: {p.get('license') or 'see its repository'}"
            + (f"; source: {p['repository']}" if p.get('repository') else ''), '']
    # A crate that builds bundled C source carries that source's licence in its vendor folder (libwebp's).
    texts += [os.path.join('vendor', n) for n in sorted(os.listdir(os.path.join(folder, 'vendor'))) if n.upper().startswith(('LICENSE', 'COPYING', 'PATENTS'))] \
        if os.path.isdir(os.path.join(folder, 'vendor')) else []
    shipped = [n for n in texts if os.path.isfile(os.path.join(folder, n))]
    for name in shipped:
        out += [f'{name}:', '', '```', open(os.path.join(folder, name), encoding='utf-8', errors='replace').read().strip(), '```', '']
    if not shipped:
        out += ['The crate ships no licence file: its licence is the one it declares, whose standard text is at the end.', '']
        missing.add(p.get('license') or '')

# The standard text of each licence a crate declares without shipping it.
standard = {
    'MIT': MIT,
    'Apache-2.0': open(os.path.join(root, 'LICENSE-APACHE'), encoding='utf-8').read(),
    'MPL-2.0': next((open(f, encoding='utf-8').read() for f in glob.glob(os.path.join(os.path.dirname(next(
        p['manifest_path'] for p in packages if p['name'] == 'cssparser')), 'LICENSE'))), ''),
    'Zlib': ZLIB,
}
named = {part.strip('() ') for expr in missing for part in expr.replace(' OR ', '/').replace(' AND ', '/').split('/')}
out += ['## Licence texts', '', 'The standard text of each licence named above by a crate that ships no licence file.', '']
for name in [n for n in standard if n in named]:
    out += [f'### {name}', '', '```', standard[name].strip(), '```', '']
# As UTF-8 bytes: on Windows, Python would otherwise write the console's code page.
sys.stdout.buffer.write(('\n'.join(out) + '\n').encode('utf-8'))
