# Makes the npm packages for a release from its archives: @ifloom/twee-tools, whose `twee-tools` command runs the build
# for the system it's on, and one package per system holding that build, which npm installs by its os and cpu.
#
#   python3 scripts/npm-packages.py --version 0.1.1 --dist dist --out npm-out [--partial]
#
# --dist holds the release's archives (twee-tools-<target>.tar.gz / .zip); --partial leaves out systems whose archive
# isn't there, for trying it with one build.
import argparse, json, os, shutil, sys, tarfile, zipfile

root = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')
REPO = 'https://github.com/IFLoom/twee-tools'

# Release target, npm package suffix, os, cpus.
SYSTEMS = [
    ('x86_64-unknown-linux-musl', 'linux-x64', 'linux', ['x64']),
    ('aarch64-unknown-linux-musl', 'linux-arm64', 'linux', ['arm64']),
    ('x86_64-apple-darwin', 'darwin-x64', 'darwin', ['x64']),
    ('aarch64-apple-darwin', 'darwin-arm64', 'darwin', ['arm64']),
    # Windows on Arm runs the x64 build, so it installs this one too.
    ('x86_64-pc-windows-msvc', 'win32-x64', 'win32', ['x64', 'arm64']),
]

args = argparse.ArgumentParser()
args.add_argument('--version', required=True)
args.add_argument('--dist', required=True)
args.add_argument('--out', required=True)
args.add_argument('--partial', action='store_true')
args = args.parse_args()

def common(name, description):
    return {
        'name': name,
        'version': args.version,
        'description': description,
        'license': 'MIT OR Apache-2.0',
        'homepage': 'https://ifloom.dev/twee-tools',
        'repository': {'type': 'git', 'url': f'git+{REPO}.git', 'directory': 'npm/twee-tools'},
        'bugs': f'{REPO}/issues',
        'publishConfig': {'access': 'public'},
    }

def write(folder, package):
    with open(os.path.join(folder, 'package.json'), 'w', encoding='utf-8') as f:
        json.dump(package, f, indent=2)
        f.write('\n')

shutil.rmtree(args.out, ignore_errors=True)
made = []
for target, suffix, system, cpus in SYSTEMS:
    exe = '.exe' if system == 'win32' else ''
    archive = os.path.join(args.dist, f'twee-tools-{target}' + ('.zip' if exe else '.tar.gz'))
    if not os.path.isfile(archive):
        if args.partial:
            print(f'leaving out {suffix}: no {archive}', file=sys.stderr)
            continue
        sys.exit(f'{archive} is missing')
    name = f'@ifloom/twee-tools-{suffix}'
    folder = os.path.join(args.out, f'twee-tools-{suffix}')
    os.makedirs(os.path.join(folder, 'bin'))
    # The archive's one folder: the binary, its licences and the third-party notices.
    inner = f'twee-tools-{target}/'
    if exe:
        with zipfile.ZipFile(archive) as z:
            files = {n[len(inner):]: z.read(n) for n in z.namelist() if n.startswith(inner) and not n.endswith('/')}
    else:
        with tarfile.open(archive) as t:
            files = {m.name[len(inner):]: t.extractfile(m).read() for m in t.getmembers() if m.isfile() and m.name.startswith(inner)}
    for file in ['LICENSE-MIT', 'LICENSE-APACHE', 'THIRD-PARTY-NOTICES.md']:
        with open(os.path.join(folder, file), 'wb') as f:
            f.write(files[file])
    binary = os.path.join(folder, 'bin', f'twee-tools{exe}')
    with open(binary, 'wb') as f:
        f.write(files[f'twee-tools{exe}'])
    os.chmod(binary, 0o755)
    with open(os.path.join(folder, 'README.md'), 'w', encoding='utf-8') as f:
        f.write(f'# {name}\n\nThe twee-tools build for {system} on {" and ".join(cpus)}. Install '
                '[@ifloom/twee-tools](https://www.npmjs.com/package/@ifloom/twee-tools) instead, which installs this '
                'for you.\n')
    package = common(name, f'The twee-tools build for {system} on {" and ".join(cpus)}: install @ifloom/twee-tools instead.')
    package.update({'os': [system], 'cpu': cpus, 'files': ['bin', 'LICENSE-MIT', 'LICENSE-APACHE', 'THIRD-PARTY-NOTICES.md']})
    write(folder, package)
    made.append(name)

# The main package: the launcher and its README, with every system's package as an optional dependency.
main = os.path.join(args.out, 'twee-tools')
shutil.copytree(os.path.join(root, 'npm', 'twee-tools'), main)
for file in ['LICENSE-MIT', 'LICENSE-APACHE']:
    shutil.copy(os.path.join(root, file), main)
package = common('@ifloom/twee-tools', 'A Twee compiler, story checker and media compressor for Twine stories, in one program.')
package.update({
    'keywords': ['twine', 'twee', 'tweego', 'sugarcube', 'harlowe', 'chapbook', 'snowman', 'interactive-fiction', 'compiler', 'linter'],
    'bin': {'twee-tools': 'bin/twee-tools.js'},
    'files': ['bin', 'LICENSE-MIT', 'LICENSE-APACHE'],
    'engines': {'node': '>=18'},
    'optionalDependencies': {f'@ifloom/twee-tools-{suffix}': args.version for _, suffix, _, _ in SYSTEMS},
})
write(main, package)
print('\n'.join(['made', '@ifloom/twee-tools', *made]))
