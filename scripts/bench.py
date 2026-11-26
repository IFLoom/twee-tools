# Times twee-tools against Tweego, twee-ts and Extwee: python3 scripts/bench.py <bench dir> <story folder>...
# <bench dir> holds tweego/ (the Tweego 2.1.1 release unzipped) and node/ (npm install @rohal12/twee-ts extwee).
# Stories: real projects, or ones made by scripts/synthetic.py. RUNS=n sets the runs per tool (default 7).
import os, subprocess, statistics, sys, time, glob, shutil
B = sys.argv[1]
TT = os.path.expanduser('~/work/personal/twee-tools/target/release/twee-tools')
TWEEGO = f'{B}/tweego/tweego'
NODE_BIN = f'{B}/node/node_modules/.bin'
FORMAT_DIR = f'{B}/tweego/storyformats'
FORMAT_JS = f'{FORMAT_DIR}/sugarcube-2/format.js'
env = dict(os.environ, TWEEGO_PATH=FORMAT_DIR, NO_COLOR='1')
OUT = f'{B}/out'; os.makedirs(OUT, exist_ok=True)

def merged(corpus):
    """One Twee file for Extwee: every .twee, then the JS and CSS as script and stylesheet passages."""
    path = f'{B}/merged-{os.path.basename(corpus)}.twee'
    if not os.path.exists(path):
        parts = []
        for f in sorted(glob.glob(f'{corpus}/**/*.twee', recursive=True)):
            parts.append(open(f, encoding='utf-8').read())
        for f in sorted(glob.glob(f'{corpus}/**/*.js', recursive=True)):
            parts.append(f':: {os.path.basename(f)} [script]\n' + open(f, encoding='utf-8').read())
        for f in sorted(glob.glob(f'{corpus}/**/*.css', recursive=True)):
            parts.append(f':: {os.path.basename(f)} [stylesheet]\n' + open(f, encoding='utf-8').read())
        open(path, 'w', encoding='utf-8').write('\n\n'.join(parts))
    return path

def tools(corpus):
    name = os.path.basename(corpus)
    return [
        ('twee-tools (compile)', [TT, 'build', corpus, '-o', f'{OUT}/{name}-tt.html', '--no-check', '--format', 'sugarcube-2', '--color', 'never']),
        ('twee-tools check', [TT, 'check', corpus, '--color', 'never']),
        ('Tweego 2.1.1', [TWEEGO, '-o', f'{OUT}/{name}-tweego.html', corpus]),
        ('twee-ts 2.1.0', [f'{NODE_BIN}/twee-ts', '--no-config', '--no-remote', '-f', 'sugarcube-2', '-o', f'{OUT}/{name}-tts.html', corpus]),
        ('Extwee 2.4.3', [f'{NODE_BIN}/extwee', '-c', '-i', merged(corpus), '-s', FORMAT_JS, '-o', f'{OUT}/{name}-extwee.html']),
    ]

def run(cmd):
    t = time.perf_counter()
    p = subprocess.run(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    return time.perf_counter() - t, p.returncode, p.stderr.decode(errors='replace')

def peak_kb(cmd):
    p = subprocess.run(['/usr/bin/time', '-f', '%M'] + cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    return int(p.stderr.decode().strip().splitlines()[-1])

runs = int(os.environ.get('RUNS', '7'))
for corpus in sys.argv[2:]:
    print(f'\n== {os.path.basename(corpus)}')
    for label, cmd in tools(corpus):
        _, code, err = run(cmd)  # warm-up
        if code not in (0, 1) or ('check' not in label and code != 0):
            print(f'  {label:22} failed (exit {code}): {err.strip().splitlines()[-1][:120] if err.strip() else ""}')
            continue
        times = [run(cmd)[0] for _ in range(runs)]
        kb = peak_kb(cmd)
        print(f'  {label:22} median {statistics.median(times)*1000:8.1f} ms   min {min(times)*1000:8.1f} ms   peak memory {kb/1024:6.1f} MB')
