// Plays every passage of a SugarCube story in headless Chrome and records, for each, the images it shows: whether each
// loaded and its natural size, plus anything the page threw. Run on an original and its compressed copy, then compare:
//
//   node scripts/same-images.mjs <story.html> > a.json
//
// Images are compared by path without the extension, so a.png and a.webp are the same picture.
import { spawn } from 'node:child_process'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

const file = process.argv[2]
const chrome = process.env.CHROME ?? `${process.env.HOME}/.cache/ms-playwright/chromium_headless_shell-1243/chrome-headless-shell-linux64/chrome-headless-shell`
const profile = mkdtempSync(path.join(tmpdir(), 'same-images-'))
const browser = spawn(chrome, ['--no-sandbox', '--allow-file-access-from-files', '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank'], { stdio: ['ignore', 'ignore', 'pipe'] })
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))

try {
  const wsUrl = await new Promise((resolve, reject) => {
    let text = ''
    browser.stderr.on('data', (chunk) => {
      text += chunk
      const found = text.match(/DevTools listening on (ws:\/\/\S+)/)
      if (found) resolve(found[1])
    })
    setTimeout(() => reject(new Error('The browser did not start in 20 s.')), 20000)
  })
  const port = new URL(wsUrl).port
  const target = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()).find((t) => t.type === 'page')
  const socket = new WebSocket(target.webSocketDebuggerUrl)
  await new Promise((resolve) => socket.addEventListener('open', resolve, { once: true }))
  let id = 0
  const pending = new Map()
  const loaded = []
  const thrown = []
  socket.addEventListener('message', (event) => {
    const message = JSON.parse(event.data)
    if (message.id && pending.has(message.id)) {
      const { resolve, reject } = pending.get(message.id)
      pending.delete(message.id)
      message.error ? reject(new Error(message.error.message)) : resolve(message.result)
    } else if (message.method === 'Page.loadEventFired') loaded.splice(0).forEach((resolve) => resolve())
    else if (message.method === 'Runtime.exceptionThrown') thrown.push(message.params.exceptionDetails.exception?.description?.split('\n')[0] ?? 'error')
  })
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      pending.set(++id, { resolve, reject })
      socket.send(JSON.stringify({ id, method, params }))
    })
  const run = async (expression) => {
    const result = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true, replMode: true })
    if (result.exceptionDetails) return { threw: result.exceptionDetails.exception?.description?.split('\n')[0] }
    return result.result.value
  }
  await send('Page.enable')
  await send('Runtime.enable')
  // The same random numbers in every run, reset before each passage (below), so a passage that picks a picture picks the same
  // one in the original and the copy however long the images before it took to load.
  await send('Page.addScriptToEvaluateOnNewDocument', { source: 'window.__seed = 42; Math.random = () => ((window.__seed = (window.__seed * 16807) % 2147483647) - 1) / 2147483646;' })
  const load = new Promise((resolve) => loaded.push(resolve))
  await send('Page.navigate', { url: pathToFileURL(path.resolve(file)).href })
  await load
  await sleep(1500)
  // SugarCube's dialogs (an age gate, say) would sit over the passages; they don't stop Engine.play.
  const names = await run(`SugarCube.Story.filter((p) => !p.tags.some((t) => ['widget', 'script', 'stylesheet', 'init'].includes(t))).map((p) => p.name)`)
  const passages = {}
  for (const name of names) {
    thrown.length = 0
    const images = await run(`
      window.__seed = 42;
      SugarCube.Engine.play(${JSON.stringify(name)}, true);
      const shown = [...document.querySelectorAll('#passages img, #story-caption img, #ui-bar img')];
      const settle = (img) => img.complete ? Promise.resolve() : new Promise((done) => { img.onload = img.onerror = done; setTimeout(done, 4000) });
      await Promise.all(shown.map(settle));
      shown.map((img) => ({ src: decodeURI(img.getAttribute('src') ?? '').replace(/\\.[A-Za-z0-9]+$/, ''), ok: img.complete && img.naturalWidth > 0, w: img.naturalWidth, h: img.naturalHeight }))
    `)
    passages[name] = { images, thrown: [...new Set(thrown)] }
  }
  console.log(JSON.stringify(passages))
  socket.close()
} finally {
  browser.kill()
  await sleep(200)
  rmSync(profile, { recursive: true, force: true })
}
