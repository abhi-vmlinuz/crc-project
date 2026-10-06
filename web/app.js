/**
 * CRC Lab UI — textbook version.
 * Inputs are binary strings. Everything recomputes live.
 */

import { parseBits, textToBits, bitsToString, validBits, divide, buildFrame, checkFrame, buildTexReport } from './crc.js';

const $ = (id) => document.getElementById(id);

const state = {
  dataMode: 'text', // 'text' | 'bits'
  dataText: 'HELLO',
  dataBitsText: '1011001',
  genText: '1011',
  data: parseBits('1011001'),
  gen: parseBits('1011'),
  frame: [],
  crc: [],
  received: [],
  damaged: new Set(),
  divSteps: [],
  divAugmented: [],
  step: 0,
  playing: false,
  timer: null,
};

function field(label, value, className = '') {
  return `<dt>${label}</dt><dd class="${className}">${value}</dd>`;
}

function escapeHtml(text) {
  return String(text).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
}

function currentDataBits() {
  if (state.dataMode === 'text') {
    if (!state.dataText.length || state.dataText.length > 12) return null;
    return textToBits(state.dataText);
  }
  if (!validBits(state.dataBitsText, 1, 64)) return null;
  return parseBits(state.dataBitsText);
}

function rebuild() {
  stopPlayback();
  const dataBits = currentDataBits();
  const dataOk = dataBits !== null && dataBits.length > 0;
  const genOk = validBits(state.genText, 2, 17);
  $('data-hint').textContent = dataOk
    ? ''
    : state.dataMode === 'text'
      ? 'Type 1–12 characters.'
      : 'Use only 0 and 1 (1–64 bits).';
  $('gen-hint').textContent = genOk
    ? ''
    : 'Generator must be binary, 2–17 bits, starting and ending with 1 (e.g. 1011).';

  if (!dataOk || !genOk) {
    state.frame = [];
    state.received = [];
    state.divSteps = [];
    return;
  }
  // Generator must start and end with 1 to be a real polynomial.
  const gen = parseBits(state.genText);
  if (gen[0] !== 1 || gen[gen.length - 1] !== 1) {
    $('gen-hint').textContent = 'Generator must start and end with 1 (e.g. 1011, 1001).';
    state.frame = [];
    state.received = [];
    state.divSteps = [];
    return;
  }
  state.data = dataBits;
  state.gen = gen;

  const built = buildFrame(state.data, state.gen);
  state.frame = built.frame;
  state.crc = built.crcBits;
  state.received = built.frame.slice();
  state.damaged.clear();
  const { steps, augmented } = divide(state.data, state.gen);
  state.divSteps = steps;
  state.divAugmented = augmented;
  state.step = 0;
}

function renderInputs() {
  const n = state.gen.length;
  $('inputs-tag').textContent = state.frame.length ? `${state.data.length} data + ${n - 1} CRC` : 'invalid';
  if (!state.frame.length) {
    $('tx-fields').innerHTML = '';
    return;
  }
  $('tx-fields').innerHTML = [
    field('data', bitsToString(state.data)),
    field('generator', bitsToString(state.gen)),
    field('appended zeros', String(n - 1)),
    field('CRC remainder', bitsToString(state.crc), 'val'),
    field('frame (data+CRC)', bitsToString(state.frame), 'hl'),
  ].join('');
}

function renderTx() {
  const split = state.data.length;
  $('frame-tag').textContent = state.frame.length ? `${state.frame.length} bits` : '—';
  $('tx-ribbon').innerHTML = state.frame
    .map((bit, i) => {
      const classes = ['bit'];
      if (bit) classes.push('one');
      if (i >= split) classes.push('check');
      return `<span class="${classes.join(' ')}" title="bit ${i}">${bit}</span>`;
    })
    .join('');
}

function renderDivision() {
  const steps = state.divSteps;
  if (!steps.length) {
    $('division').innerHTML = '<p class="note">Fix the inputs to see the division.</p>';
    $('step-label').textContent = 'step 0 / 0';
    $('division-tag').textContent = '—';
    return;
  }
  // Classic schoolbook layout, like decimal long division: quotient on top,
  // dividend below, and one staggered subtraction row per step. The only
  // difference from ordinary division is that "subtract" means XOR (in binary
  // arithmetic without carries, subtraction and XOR are the same thing, so
  // there are never any borrows).
  const quotient = steps.map((s) => (s.xored ? '1' : '0')).join('');
  const dividend = bitsToString(state.divAugmented);
  let html = '<pre>';
  html += `<span class="row-idle">quotient  ${escapeHtml(quotient)}</span>\n`;
  html += `<span class="dividend">divisor ${escapeHtml(bitsToString(state.gen))} ⟌ ${escapeHtml(dividend)}</span>\n`;

  // Show subtraction rows up to and including the current step.
  const upto = state.step;
  for (let i = 0; i <= upto; i += 1) {
    const st = steps[i];
    if (!st.xored) continue;
    const shifted = ' '.repeat(8 + st.index) + bitsToString(state.gen);
    const result = ' '.repeat(8 + st.index) + bitsToString(st.bits.slice(st.index));
    const cls = i === state.step ? 'row-active' : 'row-xor';
    html += `<span class="${cls}">      ${escapeHtml(shifted)}\n`;
    html += `      ${'─'.repeat(state.gen.length)}\n`;
    html += `      ${escapeHtml(result)}</span>\n`;
  }
  html += '</pre>';
  $('division').innerHTML = html;
  $('scrub').max = String(Math.max(0, steps.length - 1));
  $('scrub').value = String(state.step);
  $('step-label').textContent = `step ${steps.length ? state.step + 1 : 0} / ${steps.length}`;
  const active = steps[state.step];
  $('division-tag').textContent = active
    ? active.xored
      ? `subtract (XOR) at bit ${active.index}`
      : `leading 0 at bit ${active.index} — nothing to subtract`
    : '—';
}

function renderRx() {
  const split = state.data.length;
  $('rx-ribbon').innerHTML = state.received
    .map((bit, i) => {
      const classes = ['bit'];
      if (state.damaged.has(i)) classes.push('damaged');
      else {
        if (bit) classes.push('one');
        if (i >= split) classes.push('check');
      }
      return `<span class="${classes.join(' ')}" data-pos="${i}" title="bit ${i} — click to flip">${bit}</span>`;
    })
    .join('');

  if (!state.received.length) {
    $('verdict-box').innerHTML = '';
    $('rx-fields').innerHTML = '';
    return;
  }
  const report = checkFrame(state.received, state.gen);
  const tag = $('verdict-tag');
  const status = $('status');
  if (report.valid) {
    tag.textContent = 'valid';
    tag.className = 'tag ok';
    status.className = 'status ok';
    $('status-text').textContent = 'no error';
  } else {
    tag.textContent = 'error';
    tag.className = 'tag bad';
    status.className = 'status bad';
    $('status-text').textContent = 'error detected';
  }
  const damagedList = [...state.damaged].sort((a, b) => a - b);
  $('verdict-box').innerHTML = `
    <div class="verdict ${report.valid ? 'valid' : 'error'}">
      <div class="headline">${report.valid ? 'Valid — no error' : 'Error detected'}</div>
      <p>${report.valid
        ? `Receiver divided by ${bitsToString(state.gen)} and got remainder ${bitsToString(report.remainder)} (all zero).`
        : `Receiver divided by ${bitsToString(state.gen)} and got remainder ${bitsToString(report.remainder)} (non-zero), so a bit was changed in transit.`}</p>
    </div>`;
  $('rx-fields').innerHTML = [
    field('flipped bits', damagedList.length ? damagedList.join(', ') : 'none', damagedList.length ? 'val' : ''),
    field('receiver remainder', bitsToString(report.remainder), report.valid ? 'hl' : 'val'),
  ].join('');
}

function render() {
  renderInputs();
  renderTx();
  renderDivision();
  renderRx();
}

function stopPlayback() {
  state.playing = false;
  if (state.timer) {
    clearInterval(state.timer);
    state.timer = null;
  }
  const btn = $('play');
  if (btn) btn.textContent = 'Play';
}

function togglePlayback() {
  if (state.playing) {
    stopPlayback();
    return;
  }
  if (!state.divSteps.length) return;
  state.playing = true;
  $('play').textContent = 'Pause';
  state.timer = setInterval(() => {
    if (state.step + 1 >= state.divSteps.length) {
      // Stop at the end instead of looping: restarting at 1/32 looks broken.
      stopPlayback();
    } else {
      state.step += 1;
    }
    renderDivision();
  }, 400);
}

function setMode(mode) {
  state.dataMode = mode;
  $('mode-text').className = mode === 'text' ? 'primary' : '';
  $('mode-bits').className = mode === 'bits' ? 'primary' : '';
  $('data-text-wrap').style.display = mode === 'text' ? '' : 'none';
  $('data-bits-wrap').style.display = mode === 'bits' ? '' : 'none';
  rebuild();
  render();
}

$('mode-text').addEventListener('click', () => setMode('text'));
$('mode-bits').addEventListener('click', () => setMode('bits'));
$('data-text').addEventListener('input', (e) => {
  state.dataText = e.target.value;
  rebuild();
  render();
});
$('data').addEventListener('input', (e) => {
  state.dataBitsText = e.target.value;
  rebuild();
  render();
});
$('gen').addEventListener('input', (e) => {
  state.genText = e.target.value;
  rebuild();
  render();
});
$('play').addEventListener('click', togglePlayback);
$('prev').addEventListener('click', () => {
  stopPlayback();
  state.step = Math.max(0, state.step - 1);
  renderDivision();
});
$('next').addEventListener('click', () => {
  stopPlayback();
  state.step = Math.min(state.divSteps.length - 1, state.step + 1);
  renderDivision();
});
$('scrub').addEventListener('input', (e) => {
  stopPlayback();
  state.step = Number(e.target.value);
  renderDivision();
});
$('rx-ribbon').addEventListener('click', (e) => {
  const cell = e.target.closest('.bit');
  if (!cell || !state.received.length) return;
  const pos = Number(cell.dataset.pos);
  state.received[pos] ^= 1;
  if (state.received[pos] === state.frame[pos]) state.damaged.delete(pos);
  else state.damaged.add(pos);
  renderRx();
});
$('flip-one').addEventListener('click', () => {
  if (!state.received.length) return;
  const pos = Math.floor(Math.random() * state.received.length);
  document.querySelector(`#rx-ribbon .bit[data-pos="${pos}"]`)?.click();
});
$('clear').addEventListener('click', () => {
  rebuild();
  render();
});
$('export').addEventListener('click', async () => {
  if (!state.frame.length) return;
  const btn = $('export');
  btn.disabled = true;
  btn.textContent = 'Building…';
  try {
    // Ask the backend to compile the PDF for exactly what is on screen.
    const body = state.dataMode === 'text'
      ? { mode: 'text', text: state.dataText, gen: state.genText, flips: [...state.damaged] }
      : { mode: 'bits', bits: state.dataBitsText, gen: state.genText, flips: [...state.damaged] };
    const res = await fetch('/api/export-pdf', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (!res.ok) throw new Error(`backend ${res.status}`);
    const blob = await res.blob();
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = 'crc-report.pdf';
    document.body.appendChild(a);
    a.click();
    setTimeout(() => {
      URL.revokeObjectURL(a.href);
      a.remove();
    }, 500);
  } catch {
    // No backend (e.g. plain file server): fall back to the .tex source so
    // the user still leaves with the full calculation.
    const report = checkFrame(state.received, state.gen);
    const messageLabel = state.dataMode === 'text' ? state.dataText : state.dataBitsText;
    const tex = buildTexReport({
      messageLabel,
      dataBits: state.data,
      genBits: state.gen,
      crcBits: state.crc,
      frame: state.frame,
      received: state.received,
      damaged: [...state.damaged].sort((a, b) => a - b),
      receiverRemainder: report.remainder,
      valid: report.valid,
      divSteps: state.divSteps,
      divAugmented: state.divAugmented,
    });
    const blob = new Blob([tex], { type: 'text/x-tex' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = 'crc-report.tex';
    document.body.appendChild(a);
    a.click();
    setTimeout(() => {
      URL.revokeObjectURL(a.href);
      a.remove();
    }, 500);
  } finally {
    btn.disabled = false;
    btn.textContent = 'Export';
  }
});

rebuild();
render();
