# Writes a SugarCube story of N passages over 50 Twee files, with links, macros and variables, for benchmarks:
#   python3 scripts/synthetic.py <folder> 20000
import os, random, sys
random.seed(7)
d, n = sys.argv[1], int(sys.argv[2])
os.makedirs(d, exist_ok=True)
words = "the a lamp door river garden quiet bright cold stone path window letter old new key".split()
files = [[] for _ in range(50)]
files[0].append(':: StoryTitle\nSynthetic\n\n:: StoryData\n{"ifid": "0C3B4B0E-6A33-4F0A-9E0A-6F1B5E2D7A11", "format": "SugarCube", "format-version": "2.30.0", "start": "P0"}\n')
files[0].append(':: StoryInit\n<<set $coins to 0>><<set $seen to {}>>\n')
for i in range(n):
    prose = ' '.join(random.choice(words) for _ in range(60))
    links = ' '.join(f'[[Go {j}|P{j}]]' for j in random.sample(range(n), 3))
    files[i % 50].append(f':: P{i} [t{i%7}]\n{prose}\n<<if $coins gte {i%5}>><<set $coins += 1>>You have $coins coins.<<else>>Nothing.<</if>>\n{links}\n')
for k, f in enumerate(files):
    open(os.path.join(d, f'part{k:02}.twee'), 'w').write('\n'.join(f))
open(os.path.join(d, 'story.js'), 'w').write('setup.add = function (a, b) { return a + b; };\n' * 20)
open(os.path.join(d, 'story.css'), 'w').write('body { color: #222; }\n' * 20)
