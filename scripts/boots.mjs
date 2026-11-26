// Loads a built story in headless Chrome and reports whether it started: its title, the format's engine, the passage
// shown (SugarCube), the length of the text on screen, and anything the page threw.
//
//   node scripts/boots.mjs <story.html>
import { spawn } from 'node:child_process'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

const file = process.argv[2]
const chrome = process.env.CHROME ?? `${process.env.HOME}/.cache/ms-playwright/chromium_headless_shell-1243/chrome-headless-shell-linux64/chrome-headless-shell`
const profile = mkdtempSync(path.join(tmpdir(), 'boots-'))
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
  const thrown = []
  const dialogs = []
  const loaded = []
  socket.addEventListener('message', (event) => {
    const message = JSON.parse(event.data)
    if (message.id && pending.has(message.id)) {
      const { resolve, reject } = pending.get(message.id)
      pending.delete(message.id)
      message.error ? reject(new Error(message.error.message)) : resolve(message.result)
    } else if (message.method === 'Page.loadEventFired') loaded.splice(0).forEach((resolve) => resolve())
    else if (message.method === 'Runtime.exceptionThrown') thrown.push(message.params.exceptionDetails.exception?.description?.split('\n')[0] ?? 'error')
    // An alert() would hold the page (SugarCube raises one when the story's JavaScript fails): note it and dismiss it.
    else if (message.method === 'Page.javascriptDialogOpening') {
      dialogs.push(message.params.message.split('\n').slice(0, 2).join(' ').slice(0, 200))
      socket.send(JSON.stringify({ id: ++id, method: 'Page.handleJavaScriptDialog', params: { accept: true } }))
    }
  })
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      pending.set(++id, { resolve, reject })
      socket.send(JSON.stringify({ id, method, params }))
    })
  await send('Page.enable')
  await send('Runtime.enable')
  const load = new Promise((resolve) => loaded.push(resolve))
  await send('Page.navigate', { url: pathToFileURL(path.resolve(file)).href })
  await Promise.race([load, sleep(15000)])
  await sleep(2000)
  const result = await send('Runtime.evaluate', {
    returnByValue: true,
    expression: `({
      title: document.title,
      engine: window.SugarCube ? 'SugarCube ' + SugarCube.version.short() : window.Harlowe || document.querySelector('tw-story') ? 'Harlowe' : window.story ? 'Snowman' : document.querySelector('.page') ? 'Chapbook?' : 'unknown',
      passage: window.SugarCube ? SugarCube.State.passage : (document.querySelector('tw-passage')?.getAttribute('name') ?? null),
      text: (document.querySelector('#passages, tw-story, #main, main, body')?.innerText ?? '').trim().length,
    })`,
  })
  console.log(JSON.stringify({ ...result.result.value, thrown, dialogs }))
  socket.close()
} finally {
  browser.kill()
  await sleep(200)
  rmSync(profile, { recursive: true, force: true })
}
