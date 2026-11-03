/* Krysion design language: shared behaviour. © Ken “Krysion” Nisaka. */
(() => {
	'use strict';
	if (window.Krysion) return;

	const vw = () => document.documentElement.clientWidth || innerWidth, vh = () => document.documentElement.clientHeight || innerHeight;
	const reduced = () => matchMedia('(prefers-reduced-motion: reduce)').matches;
	const EASE = 'cubic-bezier(.2, .8, .2, 1)';
	const SPRING = 'cubic-bezier(.34, 1.36, .64, 1)';
	const ON = 'button.on, button[aria-selected="true"], button[aria-pressed="true"]';

	let tip = null, tipTimer = 0, tipGoneAt = 0;
	function showTip(target) {
		clearTimeout(tipTimer);
		const instant = tip || Date.now() - tipGoneAt < 300;
		const go = () => {
			hideTip(true);
			const text = target.getAttribute('data-tip'); if (!text) return;
			tip = document.createElement('div'); tip.className = 'tooltip' + (instant ? ' instant show' : ''); tip.setAttribute('role', 'tooltip'); tip.textContent = text;
			document.body.appendChild(tip);
			const r = target.getBoundingClientRect(), w = tip.offsetWidth, h = tip.offsetHeight;
			let top = r.top - h - 8; if (top < 8) top = r.bottom + 8;
			tip.style.left = Math.max(8, Math.min(vw() - w - 8, r.left + r.width / 2 - w / 2)) + 'px';
			tip.style.top = top + 'px';
			if (!instant) requestAnimationFrame(() => tip && tip.classList.add('show'));
		};
		if (instant) go(); else tipTimer = setTimeout(go, 400);
	}
	function hideTip(quiet) { clearTimeout(tipTimer); if (tip) { tip.remove(); tip = null; if (!quiet) tipGoneAt = Date.now(); } }

	const TITLED = '[title]:not(iframe, abbr, svg, svg *, link, style, meta)';
	const adopt = el => {
		const t = el.closest && el.closest(TITLED); if (!t) return;
		const v = t.getAttribute('title'); t.removeAttribute('title'); if (!v) return;
		t.setAttribute('data-tip', v);
		if (!t.hasAttribute('aria-label') && !t.textContent.trim()) t.setAttribute('aria-label', v);
	};

	const needsTip = t => { if (t.hasAttribute('data-tip-always')) return true; const words = (t.innerText || '').trim(); return !words || t.scrollWidth > t.clientWidth + 1; };
	document.addEventListener('pointerover', e => { adopt(e.target); if (e.buttons) return; const t = e.target.closest && e.target.closest('[data-tip]'); if (t && e.pointerType !== 'touch' && needsTip(t)) showTip(t); });
	document.addEventListener('pointerout', e => { const t = e.target.closest && e.target.closest('[data-tip]'); if (t && !t.contains(e.relatedTarget)) hideTip(); });
	document.addEventListener('focusin', e => { adopt(e.target); const t = e.target.closest && e.target.closest('[data-tip]'); if (t && t.matches(':focus-visible') && needsTip(t)) showTip(t); });
	document.addEventListener('focusout', () => hideTip());
	document.addEventListener('pointerdown', () => hideTip(true));
	addEventListener('scroll', () => hideTip(true), { passive: true, capture: true });

	function initSeg(seg) {
		if (seg._k || seg.hasAttribute('data-static')) return; seg._k = 1;
		seg.classList.add('seg-slide');
		const ind = document.createElement('span'); ind.className = 'seg-ind'; ind.setAttribute('aria-hidden', 'true'); seg.prepend(ind);
		let cur = null;
		const pos = b => ({ x: b.offsetLeft, y: b.offsetTop, w: b.offsetWidth, h: b.offsetHeight });
		const place = b => { if (!b) { ind.style.opacity = '0'; return; } const p = pos(b); ind.style.opacity = '1'; ind.style.width = p.w + 'px'; ind.style.height = p.h + 'px'; ind.style.transform = `translate(${p.x}px, ${p.y}px)`; };
		const sync = () => {
			if (!ind.isConnected) { seg.prepend(ind); cur = null; }
			const b = seg.querySelector(':scope > ' + ON.split(', ').join(', :scope > '));
			if (b === cur) { place(b); return; }
			const from = cur && cur.isConnected ? pos(cur) : null; cur = b; place(b);
			if (!from || !b || reduced()) return;
			const to = pos(b);
			if (from.y !== to.y) { ind.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 150 }); return; }
			const L = Math.min(from.x, to.x), R = Math.max(from.x + from.w, to.x + to.w);
			ind.animate([
				{ transform: `translate(${from.x}px, ${to.y}px)`, width: from.w + 'px' },
				{ transform: `translate(${L}px, ${to.y}px)`, width: (R - L) + 'px', offset: .45 },
				{ transform: `translate(${to.x}px, ${to.y}px)`, width: to.w + 'px' }
			], { duration: 320, easing: EASE });
		};
		new MutationObserver(sync).observe(seg, { subtree: true, attributes: true, attributeFilter: ['class', 'aria-selected', 'aria-pressed'], childList: true });
		new ResizeObserver(() => place(cur)).observe(seg);
		sync();
	}

	function initSlider(input) {
		if (input._k) return; input._k = 1;
		const wrap = document.createElement('span'); wrap.className = 'slider-wrap';
		input.parentNode.insertBefore(wrap, input); wrap.appendChild(input);
		const bub = document.createElement('span'); bub.className = 'slider-bub'; bub.setAttribute('aria-hidden', 'true'); wrap.appendChild(bub);
		const min = () => +input.min || 0, max = () => (input.max === '' ? 100 : +input.max);
		const pct = () => (input.value - min()) / (max() - min()) * 100;
		const set = () => { const p = pct(); input.style.setProperty('--pct', p + '%'); wrap.style.setProperty('--pct', `calc(${p}% + ${(.5 - p / 100) * 24}px)`); const dp = input.dataset.digits, v = num(+input.value, dp == null ? undefined : +dp); bub.textContent = input.dataset.unit ? v + input.dataset.unit : v; draw(); };
		let held = false, amp = 0, phase = 0, raf = 0, path = null;
		const sound = input.hasAttribute('data-sound');
		if (sound) {
			const ns = 'http://www.w3.org/2000/svg', svg = document.createElementNS(ns, 'svg'); svg.setAttribute('class', 'slider-wave'); svg.setAttribute('aria-hidden', 'true');
			path = document.createElementNS(ns, 'path'); svg.appendChild(path); wrap.insertBefore(svg, input);
		}
		function draw() {
			if (!path) return;
			const W = wrap.clientWidth, cy = wrap.clientHeight / 2, fx = pct() / 100 * W, sw = held ? 10 : 6;
			input.style.setProperty('--fill', amp > .05 ? 'transparent' : 'var(--accent)');
			let d = `M0 ${cy}`;
			for (let x = 2; x <= fx; x += 2) d += ` L${x} ${(cy + amp * Math.sin(x / 10 - phase) * Math.min(1, x / 14, (fx - x) / 14 + .15)).toFixed(2)}`;
			path.setAttribute('d', fx < 2 ? `M0 ${cy}` : d); path.setAttribute('stroke-width', sw);
		}
		function loop() {
			const live = held || input.dataset.playing === 'true';
			const target = live ? 1.2 + pct() / 100 * 1.6 : 0;
			amp += (target - amp) * .08; phase += .07;
			draw();
			if (live || amp > .04) raf = requestAnimationFrame(loop); else { amp = 0; draw(); raf = 0; }
		}
		const kick = () => { if (sound && !raf && !reduced()) raf = requestAnimationFrame(loop); };
		input.addEventListener('input', set);
		input.addEventListener('pointerdown', () => { held = true; input.classList.add('held'); kick(); });
		const up = () => { if (!held) return; held = false; input.classList.remove('held'); };
		addEventListener('pointerup', up); addEventListener('pointercancel', up);
		if (sound) new MutationObserver(kick).observe(input, { attributes: true, attributeFilter: ['data-playing'] });
		new ResizeObserver(draw).observe(wrap);
		set(); kick();
	}

	const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];
	const iso = d => d.getFullYear() + '-' + String(d.getMonth() + 1).padStart(2, '0') + '-' + String(d.getDate()).padStart(2, '0');
	const parse = v => { const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(v || ''); return m ? new Date(+m[1], m[2] - 1, +m[3]) : null; };
	const phone = () => matchMedia('(max-width: 600px)').matches;
	let openCal = null;
	function initDate(input) {
		if (input._k || input.hasAttribute('data-native')) return; input._k = 1;
		const btn = document.createElement('button'); btn.type = 'button'; btn.className = 'select date-btn'; btn.setAttribute('aria-haspopup', 'dialog'); btn.setAttribute('aria-expanded', 'false');
		btn.innerHTML = '<i class="fa-regular fa-calendar" aria-hidden="true"></i><span></span>';
		const label = input.id && document.querySelector(`label[for="${CSS.escape(input.id)}"]`);
		if (label) { label.addEventListener('click', e => { e.preventDefault(); btn.focus(); }); btn.setAttribute('aria-label', label.textContent.trim()); }
		else if (input.getAttribute('aria-label')) btn.setAttribute('aria-label', input.getAttribute('aria-label'));
		input.style.display = 'none'; input.after(btn);
		const pop = document.createElement('div'); pop.className = 'cal date-pop'; pop.setAttribute('role', 'dialog'); pop.setAttribute('aria-label', 'Pick a date');
		const scrim = document.createElement('div'); scrim.className = 'date-scrim';
		const st = { y: 0, m: 0, view: 'days', focus: null };

		const desc = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value');
		Object.defineProperty(input, 'value', { configurable: true, get() { return desc.get.call(this); }, set(v) { desc.set.call(this, v); show(); } });
		const lim = () => [parse(input.min), parse(input.max)];
		const off = d => { const [lo, hi] = lim(); return (lo && d < lo) || (hi && d > hi); };
		function show() {
			const d = parse(input.value);
			btn.lastChild.textContent = d ? date(d, 'short') : (input.placeholder || input.dataset.placeholder || 'Pick a date');
			btn.toggleAttribute('data-empty', !d);
			btn.disabled = input.disabled;
		}
		function draw() {
			const now = new Date(), today = new Date(now.getFullYear(), now.getMonth(), now.getDate()), sel = parse(input.value);
			let body;
			if (st.view === 'months') body = `<div class="cal-months">${MONTHS.map((n, i) => `<button type="button" data-m="${i}" class="${i === st.m ? 'on' : ''}">${n.slice(0, 3)}</button>`).join('')}</div>`;
			else {
				const first = (new Date(st.y, st.m, 1).getDay() + 6) % 7;
				let cells = ['M', 'T', 'W', 'T', 'F', 'S', 'S'].map(w => `<span class="cal-wd" aria-hidden="true">${w}</span>`).join('');
				for (let i = 0; i < 42; i++) {
					const d = new Date(st.y, st.m, 1 - first + i), c = ['cal-d'], t = d.getTime();
					if (d.getMonth() !== st.m) c.push('out'); if (t === today.getTime()) c.push('today'); if (sel && t === sel.getTime()) c.push('start', 'end');
					cells += `<span class="${c.join(' ')}"><button type="button" data-d="${t}" tabindex="${st.focus && t === st.focus.getTime() ? 0 : -1}"${off(d) ? ' disabled' : ''} aria-label="${date(d)}">${d.getDate()}</button></span>`;
				}
				body = `<div class="cal-grid">${cells}</div>`;
			}
			const clear = !input.required && input.value ? '<button type="button" class="btn btn-text btn-sm" data-clear>Clear</button>' : '';
			pop.innerHTML = `<div class="cal-head"><button type="button" class="icon-btn" data-nav="-1" aria-label="${st.view === 'days' ? 'Previous month' : 'Previous year'}"><i class="fa-solid fa-chevron-left" aria-hidden="true"></i></button><button type="button" class="cal-title" data-view aria-expanded="${st.view === 'months'}">${st.view === 'days' ? MONTHS[st.m] + ' ' + st.y : st.y}<i class="fa-solid fa-chevron-down" aria-hidden="true"></i></button><button type="button" class="icon-btn" data-nav="1" aria-label="${st.view === 'days' ? 'Next month' : 'Next year'}"><i class="fa-solid fa-chevron-right" aria-hidden="true"></i></button></div>${body}<div class="cal-foot">${clear}<button type="button" class="btn btn-text btn-sm" data-today>Today</button></div>`;
		}
		function place() {
			if (phone()) { pop.style.left = pop.style.top = ''; return; }
			const r = btn.getBoundingClientRect(), w = pop.offsetWidth, h = pop.offsetHeight;
			let top = r.bottom + 8; if (top + h > vh() - 8 && r.top - h - 8 > 8) top = r.top - h - 8;
			pop.style.left = Math.max(8, Math.min(vw() - w - 8, r.left)) + 'px'; pop.style.top = top + 'px';
		}
		function open() {
			if (openCal && openCal !== close) openCal();
			const d = parse(input.value) || new Date(); st.y = d.getFullYear(); st.m = d.getMonth(); st.view = 'days'; st.focus = new Date(d.getFullYear(), d.getMonth(), d.getDate());
			draw(); document.body.append(scrim, pop); pop.classList.add('open'); scrim.classList.toggle('open', phone()); btn.setAttribute('aria-expanded', 'true'); place();
			openCal = close; addEventListener('scroll', place, true); addEventListener('resize', place);
			const f = pop.querySelector('.cal-d > button[tabindex="0"]') || pop.querySelector('.cal-d > button'); if (f && matchMedia('(pointer: fine)').matches) f.focus({ preventScroll: true });
		}
		function close(refocus) {
			if (!pop.isConnected) return; pop.remove(); scrim.remove(); pop.classList.remove('open'); btn.setAttribute('aria-expanded', 'false');
			if (openCal === close) openCal = null; removeEventListener('scroll', place, true); removeEventListener('resize', place);
			if (refocus === true) btn.focus();
		}
		function pick(v) { desc.set.call(input, v); show(); input.dispatchEvent(new Event('input', { bubbles: true })); input.dispatchEvent(new Event('change', { bubbles: true })); close(true); }
		btn.addEventListener('click', () => pop.isConnected ? close() : open());
		scrim.addEventListener('click', () => close());
		pop.addEventListener('pointerdown', e => e.stopPropagation());
		document.addEventListener('pointerdown', e => { if (pop.isConnected && !btn.contains(e.target)) close(); });
		pop.addEventListener('click', e => {
			const n = e.target.closest('[data-nav]');
			if (n) { if (st.view === 'months') st.y += +n.dataset.nav; else { st.m += +n.dataset.nav; if (st.m < 0) { st.m = 11; st.y--; } if (st.m > 11) { st.m = 0; st.y++; } } draw(); place(); return; }
			if (e.target.closest('[data-view]')) { st.view = st.view === 'days' ? 'months' : 'days'; draw(); place(); const ic = pop.querySelector('.cal-title > i'); if (ic && !reduced()) ic.animate([{ transform: st.view === 'months' ? 'none' : 'rotate(180deg)' }, { transform: st.view === 'months' ? 'rotate(180deg)' : 'none' }], { duration: 200, easing: EASE }); return; }
			if (e.target.closest('[data-today]')) { const t = new Date(); if (!off(new Date(t.getFullYear(), t.getMonth(), t.getDate()))) pick(iso(t)); return; }
			if (e.target.closest('[data-clear]')) { pick(''); return; }
			const mb = e.target.closest('[data-m]'); if (mb) { st.m = +mb.dataset.m; st.view = 'days'; draw(); place(); return; }
			const db = e.target.closest('[data-d]'); if (db && !db.disabled) pick(iso(new Date(+db.dataset.d)));
		});
		pop.addEventListener('keydown', e => {
			if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); close(true); return; }
			if (st.view !== 'days' || !e.target.closest('.cal-d')) return;
			const step = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 }[e.key];
			const d = new Date(+e.target.dataset.d);
			if (step) d.setDate(d.getDate() + step); else if (e.key === 'PageUp' || e.key === 'PageDown') d.setMonth(d.getMonth() + (e.key === 'PageUp' ? -1 : 1)); else return;
			e.preventDefault(); st.focus = d; st.y = d.getFullYear(); st.m = d.getMonth(); draw();
			const f = pop.querySelector('.cal-d > button[tabindex="0"]'); if (f) f.focus();
		});
		new MutationObserver(show).observe(input, { attributes: true, attributeFilter: ['disabled', 'min', 'max', 'placeholder'] });
		show();
	}

	function reorder(list, opts = {}) {
		const handle = opts.handle || '.grip', rowSel = opts.rows || ':scope > *';
		const rows = () => [...list.querySelectorAll(rowSel)];
		const rowOf = g => rows().find(r => r.contains(g));
		list.addEventListener('pointerdown', e => {
			const g = e.target.closest(handle); if (!g || !list.contains(g) || e.button) return;
			const row = rowOf(g); if (!row) return;
			e.preventDefault();
			const all = rows(), i0 = all.indexOf(row), y0 = e.clientY;
			const H = all.length > 1 ? all[1].offsetTop - all[0].offsetTop : row.offsetHeight;
			let idx = i0;
			try { g.setPointerCapture(e.pointerId); } catch (err) {}
			row.classList.add('lifted'); row.style.position = 'relative'; row.style.zIndex = '3';
			all.forEach(r => { if (r !== row) r.style.transition = reduced() ? 'none' : `transform 220ms ${EASE}`; });
			const move = ev => {
				const dy = ev.clientY - y0; row.style.transition = 'box-shadow 200ms'; row.style.transform = `translateY(${dy}px) scale(1.02)`;
				const n = Math.max(0, Math.min(all.length - 1, Math.round(i0 + dy / H))); if (n === idx) return; idx = n;
				all.forEach((r, j) => { if (r === row) return; let s = 0; if (i0 < idx && j > i0 && j <= idx) s = -H; if (i0 > idx && j < i0 && j >= idx) s = H; r.style.transform = s ? `translateY(${s}px)` : ''; });
			};
			const end = () => {
				g.removeEventListener('pointermove', move); g.removeEventListener('pointerup', end); g.removeEventListener('pointercancel', end);
				row.classList.add('dropping'); row.style.transition = `transform 240ms ${SPRING}, box-shadow 240ms`; row.style.transform = `translateY(${(idx - i0) * H}px)`;
				setTimeout(() => {
					all.forEach(r => { r.style.transition = 'none'; r.style.transform = ''; });
					const ref = rows().filter(r => r !== row)[idx]; if (ref) list.insertBefore(row, ref); else list.appendChild(row);
					row.classList.remove('lifted', 'dropping'); row.style.position = row.style.zIndex = '';
					void list.offsetWidth; all.forEach(r => { r.style.transition = ''; });
					if (idx !== i0 && opts.onChange) opts.onChange(i0, idx, false);
				}, reduced() ? 0 : 240);
			};
			g.addEventListener('pointermove', move); g.addEventListener('pointerup', end); g.addEventListener('pointercancel', end);
		});
		list.addEventListener('keydown', e => {
			const g = e.target.closest(handle); if (!g || (e.key !== 'ArrowUp' && e.key !== 'ArrowDown')) return;
			const row = rowOf(g), all = rows(), i = all.indexOf(row), j = i + (e.key === 'ArrowUp' ? -1 : 1);
			if (!row || j < 0 || j >= all.length) return;
			e.preventDefault();
			const before = new Map(all.map(r => [r, r.offsetTop]));
			if (j < i) list.insertBefore(row, all[j]); else list.insertBefore(row, all[j].nextSibling);
			g.focus({ preventScroll: true });
			if (!reduced()) rows().forEach(r => { const dy = before.get(r) - r.offsetTop; if (dy) r.animate([{ transform: `translateY(${dy}px)` }, { transform: 'none' }], { duration: 220, easing: EASE }); });
			if (opts.onChange) opts.onChange(i, j, true);
		});
	}

	function viewer(pics, start = 0, thumbs = []) {
		if (!pics.length) return;
		let i = Math.max(0, Math.min(pics.length - 1, start)), z = 1, px = 0, py = 0, pd = null, tapTimer = 0, lastTap = 0, busy = false;
		const n = pics.length, many = n > 1, v = document.createElement('div'); v.className = 'viewer open'; v.setAttribute('role', 'dialog'); v.setAttribute('aria-modal', 'true'); v.setAttribute('aria-label', 'Picture'); v.tabIndex = -1;
		v.innerHTML = `<div class="viewer-bg"></div><img class="viewer-img" alt="" draggable="false"><div class="viewer-ui"><div class="v-top"><a class="bar-btn v-open" target="_blank" rel="noopener" aria-label="Open original" data-tip="Open original"><i class="fa-solid fa-arrow-up-right-from-square" aria-hidden="true"></i></a><button type="button" class="bar-btn v-close" aria-label="Close" data-tip="Close"><i class="fa-solid fa-xmark" aria-hidden="true"></i></button></div>${many ? '<button type="button" class="bar-btn v-prev" aria-label="Previous" data-tip="Previous"><i class="fa-solid fa-chevron-left" aria-hidden="true"></i></button><button type="button" class="bar-btn v-next" aria-label="Next" data-tip="Next"><i class="fa-solid fa-chevron-right" aria-hidden="true"></i></button>' : ''}<div class="v-foot"><span class="bar-pill static v-cap"></span>${many ? '<span class="bar-pill static v-count"></span>' : ''}</div></div>`;
		const img = v.querySelector('.viewer-img'), bg = v.querySelector('.viewer-bg'), back = document.activeElement, locked = document.body.classList.contains('modal-open');
		const fit = () => {
			const p = pics[i], W = vw(), H = vh(), phone = W <= 600, m = phone ? 16 : 72, nw = p._w || img.naturalWidth || 4, nh = p._h || img.naturalHeight || 3;

			const s = Math.min((W - m * 2) / nw, (H - m * 2 - (phone ? 64 : 0)) / nh, Math.max(2, 320 / Math.min(nw, nh))), w = nw * s, h = nh * s;
			return { l: (W - w) / 2, t: (H - h) / 2, w, h };
		};
		const place = f => Object.assign(img.style, { left: f.l + 'px', top: f.t + 'px', width: f.w + 'px', height: f.h + 'px' });
		const OPEN = 'inset(0px 0px 0px 0px round 12px)';

		const crop = f => {
			const t = thumbs[i]; if (!t || !t.isConnected) return null;
			const r = t.getBoundingClientRect(); if (!r.width || !r.height) return null;
			const cs = getComputedStyle(t), s = (/contain|scale-down/.test(cs.objectFit) ? Math.min : Math.max)(r.width / f.w, r.height / f.h);
			const [ox, oy] = (cs.objectPosition || '50% 50%').split(' ').map(x => x.endsWith('%') ? parseFloat(x) / 100 : .5);
			const w = f.w * s, h = f.h * s, x = r.left + (r.width - w) * ox, y = r.top + (r.height - h) * oy;
			const rad = el => { const b = getComputedStyle(el).borderTopLeftRadius; return b.endsWith('%') ? parseFloat(b) / 100 * Math.min(r.width, r.height) : parseFloat(b) || 0; };
			let k = rad(t);
			for (let p = t.parentElement, d = 0; !k && p && d < 2; p = p.parentElement, d++) { const q = p.getBoundingClientRect(); if (Math.abs(q.width - r.width) < 2 && Math.abs(q.height - r.height) < 2) k = rad(p); }
			const L = Math.max(0, r.left - x) / s, T = Math.max(0, r.top - y) / s, R = Math.max(0, x + w - r.right) / s, B = Math.max(0, y + h - r.bottom) / s;
			return { transform: `translate(${x - f.l}px, ${y - f.t}px) scale(${s})`, clipPath: `inset(${T}px ${R}px ${B}px ${L}px round ${k / s}px)` };
		};

		const resize = () => {
			const a = img.getBoundingClientRect(), f = fit();
			img.getAnimations().forEach(x => x.cancel()); place(f);
			if (!reduced() && a.width && Math.abs(a.width - f.w) > 1) img.animate([{ transform: `translate(${a.left - f.l}px, ${a.top - f.t}px) scale(${a.width / f.w}, ${a.height / f.h})`, clipPath: OPEN }, { transform: 'none', clipPath: OPEN }], { duration: 260, easing: EASE });
		};
		const load = () => new Promise(res => {
			const p = pics[i], k = i, first = p.thumb || p.src; img.alt = p.alt || '';
			let ready = 0, opened = false;
			const go = () => { if (!opened) { opened = true; res(); } };
			const done = () => { img.onload = img.onerror = null; if (first === p.src && img.naturalWidth) { p._w = img.naturalWidth; p._h = img.naturalHeight; } if (++ready === 2 || first === p.src || p._w) go(); };
			img.onload = img.onerror = done; img.src = first; if (img.complete && img.naturalWidth) done();
			if (first !== p.src) {
				const full = new Image();
				full.onload = () => {
					const known = !!p._w; p._w = full.naturalWidth; p._h = full.naturalHeight;
					if (i !== k || !v.isConnected) return;
					img.src = p.src;
					if (!opened) { if (++ready === 2) go(); } else if (z === 1 && !known) resize();
				};
				full.onerror = go; full.src = p.src;
				setTimeout(go, 250);
			}
		});
		const preload = () => { if (many) [i - 1, i + 1].forEach(j => { const q = pics[(j + n) % n]; if (q._w) return; const im = new Image(); im.onload = () => { q._w = im.naturalWidth; q._h = im.naturalHeight; }; im.src = q.src; }); };
		const info = () => {
			const p = pics[i], c = v.querySelector('.v-count'), cap = v.querySelector('.v-cap');
			if (c) c.textContent = `${i + 1} / ${n}`;
			cap.textContent = p.caption || ''; cap.hidden = !p.caption; v.querySelector('.v-open').href = p.src;
		};

		const setT = anim => {
			const tr = z === 1 ? '' : `translate(${px}px, ${py}px) scale(${z})`;
			if (anim && !reduced()) img.animate([{ transform: img.style.transform || 'none' }, { transform: tr || 'none' }], { duration: 260, easing: EASE });
			img.style.transform = tr; v.classList.toggle('zoomed', z > 1);
		};
		const clampPan = f => {
			const W = vw(), H = vh(), w = f.w * z, h = f.h * z;
			px = w > W ? Math.min(-f.l, Math.max(W - w - f.l, px)) : (W - w) / 2 - f.l;
			py = h > H ? Math.min(-f.t, Math.max(H - h - f.t, py)) : (H - h) / 2 - f.t;
		};
		function zoomAt(cx, cy) {
			const f = fit();
			if (z > 1) { z = 1; px = py = 0; } else { z = Math.min(4, Math.max(2, (pics[i]._w || img.naturalWidth) / f.w || 2)); px = (cx - f.l) * (1 - z); py = (cy - f.t) * (1 - z); clampPan(f); }
			setT(true);
		}

		function zoomTo(nz, cx, cy, anim) {
			const f = fit(); nz = Math.max(1, Math.min(4, nz));
			if (nz < 1.02) { z = 1; px = py = 0; } else { px = cx - f.l - (cx - f.l - px) * nz / z; py = cy - f.t - (cy - f.t - py) * nz / z; z = nz; clampPan(f); }
			setT(anim);
		}

		v.addEventListener('wheel', e => {
			if (busy || e.target.closest('.viewer-ui > *')) return;
			e.preventDefault();
			zoomTo(z * Math.exp(-e.deltaY * (e.ctrlKey ? .01 : .0025)), e.clientX, e.clientY);
		}, { passive: false });
		const springBack = () => {
			if (!reduced() && img.style.transform) img.animate([{ transform: img.style.transform }, { transform: 'none' }], { duration: 340, easing: SPRING });
			img.style.transform = '';
			if (bg.style.opacity) { bg.style.transition = 'opacity 240ms'; bg.style.opacity = ''; setTimeout(() => { bg.style.transition = ''; }, 240); }
		};
		const key = e => {
			if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); close(); }
			else if (many && e.key === 'ArrowLeft') go(-1); else if (many && e.key === 'ArrowRight') go(1);
		};

		async function go(d, dx = 0) {
			if (busy) return; busy = true;
			const old = img.cloneNode(); old.style.transform = dx ? `translateX(${dx}px)` : ''; old.removeAttribute('data-tip'); v.insertBefore(old, img);
			img.style.opacity = '0'; img.style.transform = ''; z = 1; px = py = 0; v.classList.remove('zoomed');
			i = (i + d + n) % n; info(); await load(); place(fit()); img.style.opacity = '';
			if (reduced()) { old.remove(); img.animate([{ opacity: .3 }, { opacity: 1 }], { duration: 200 }); busy = false; preload(); return; }
			const D = Math.min(vw() * .4, 260);
			old.animate([{ transform: `translateX(${dx}px)`, opacity: 1 }, { transform: `translateX(${dx - d * D}px)`, opacity: 0 }], { duration: 260, easing: EASE, fill: 'forwards' }).onfinish = () => old.remove();
			img.animate([{ transform: `translateX(${d * D}px)`, opacity: 0 }, { transform: 'none', opacity: 1 }], { duration: 260, easing: EASE });
			setTimeout(() => { busy = false; }, 200); preload();
		}
		function close() {
			if (!v.classList.contains('on')) return;
			v.classList.remove('on'); clearTimeout(tapTimer); if (!locked) document.body.classList.remove('modal-open'); removeEventListener('keydown', key, true); removeEventListener('resize', onResize);
			const c = reduced() ? null : crop(fit()), cur = img.style.transform || 'none';
			const a = c ? img.animate([{ transform: cur, clipPath: OPEN }, c], { duration: 260, easing: EASE, fill: 'forwards' }) : img.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 200, fill: 'forwards' });
			setTimeout(() => { v.remove(); if (back && back.focus) back.focus({ preventScroll: true }); }, c ? 260 : 200);
		}
		const onResize = () => { place(fit()); if (z > 1) { clampPan(fit()); setT(); } };

		function tap(e, onImg) {
			if (!onImg) { if (z === 1) close(); return; }
			if (e.pointerType !== 'touch') { zoomAt(e.clientX, e.clientY); return; }
			if (Date.now() - lastTap < 300) { clearTimeout(tapTimer); lastTap = 0; zoomAt(e.clientX, e.clientY); return; }
			lastTap = Date.now(); tapTimer = setTimeout(() => v.classList.toggle('bare'), 300);
		}

		const touches = new Map(); let pinch = null;
		const mid = () => { const [a, b] = [...touches.values()]; return { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2, d: Math.hypot(a.x - b.x, a.y - b.y) || 1 }; };
		v.addEventListener('pointerdown', e => {
			if (e.button || busy || e.target.closest('.viewer-ui > *')) return;
			e.preventDefault();
			if (e.pointerType === 'touch') touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
			if (touches.size === 2) {
				clearTimeout(tapTimer); pd = null; img.style.transform = z > 1 ? img.style.transform : ''; bg.style.opacity = '';
				const m = mid(), f = fit(); pinch = { ...m, z0: z, ux: (m.x - f.l - px) / z, uy: (m.y - f.t - py) / z };
				v.setPointerCapture(e.pointerId); return;
			}
			pd = { x: e.clientX, y: e.clientY, px, py, t: Date.now(), axis: null, onImg: e.target === img, id: e.pointerId };
			v.setPointerCapture(e.pointerId);
		});
		v.addEventListener('pointermove', e => {
			if (touches.has(e.pointerId)) touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
			if (pinch && touches.size === 2) {
				const m = mid(), f = fit(), nz = Math.max(.8, Math.min(5, pinch.z0 * m.d / pinch.d));
				z = nz; px = m.x - f.l - pinch.ux * nz; py = m.y - f.t - pinch.uy * nz;
				img.style.transform = `translate(${px}px, ${py}px) scale(${z})`; v.classList.toggle('zoomed', z > 1); return;
			}
			if (!pd || e.pointerId !== pd.id) return;
			const dx = e.clientX - pd.x, dy = e.clientY - pd.y;
			if (!pd.axis) { if (Math.hypot(dx, dy) < 8) return; pd.axis = z > 1 ? 'pan' : Math.abs(dx) > Math.abs(dy) ? (many ? 'x' : 'off') : dy > 0 ? 'y' : 'off'; }
			if (pd.axis === 'pan') { v.classList.add('panning'); px = pd.px + dx; py = pd.py + dy; img.style.transform = `translate(${px}px, ${py}px) scale(${z})`; }
			else if (pd.axis === 'x') img.style.transform = `translateX(${dx}px)`;
			else if (pd.axis === 'y') { const k = Math.max(0, dy); img.style.transform = `translate(${dx * .5}px, ${k}px) scale(${1 - Math.min(k / vh(), 1) * .25})`; bg.style.opacity = String(1 - Math.min(k / 400, .7)); }
		});
		const up = e => {
			touches.delete(e.pointerId); v.classList.remove('panning');
			if (pinch) { if (touches.size < 2) { pinch = null; pd = null; if (z < 1.02) { z = 1; px = py = 0; setT(true); } else { if (z > 4) zoomTo(4, e.clientX, e.clientY, true); else { clampPan(fit()); setT(true); } } } return; }
			if (!pd || e.pointerId !== pd.id) return;
			const p = pd, dx = e.clientX - p.x, dy = e.clientY - p.y, fast = Date.now() - p.t < 250; pd = null;
			if (e.type === 'pointercancel') { if (p.axis === 'pan') { clampPan(fit()); setT(true); } else springBack(); return; }
			if (!p.axis) tap(e, p.onImg);
			else if (p.axis === 'pan') { clampPan(fit()); setT(true); }
			else if (p.axis === 'x') { if (Math.abs(dx) > 80 || (fast && Math.abs(dx) > 30)) go(dx < 0 ? 1 : -1, dx); else springBack(); }
			else if (p.axis === 'y') { if (dy > 120 || (fast && dy > 40)) close(); else springBack(); }
		};
		v.addEventListener('pointerup', up); v.addEventListener('pointercancel', up);
		v.querySelector('.v-close').addEventListener('click', close);
		if (many) { v.querySelector('.v-prev').addEventListener('click', () => go(-1)); v.querySelector('.v-next').addEventListener('click', () => go(1)); }
		document.body.appendChild(v); info();
		load().then(() => {
			const f = fit(); place(f); void v.offsetWidth; v.classList.add('on'); document.body.classList.add('modal-open');
			const c = reduced() ? null : crop(f); if (c) img.animate([c, { transform: 'none', clipPath: OPEN }], { duration: 300, easing: EASE });
			v.focus({ preventScroll: true });
			preload();
		});
		addEventListener('keydown', key, true); addEventListener('resize', onResize);
		return { close };
	}

	function pull(opts = {}) {
		const READY = 72, REST = 60;
		let y0 = null, x0 = 0, d = 0, ready = false, busy = false, el = null, spin = null;
		const wait = ms => new Promise(r => setTimeout(r, ms));
		const can = t => !busy && scrollY <= 0 && !document.body.classList.contains('modal-open')
			&& !(t.closest && t.closest('input, textarea, select, [contenteditable], .modal, .menu, .viewer, .side-sheet, .phone-menu'))
			&& (!opts.enabled || opts.enabled(t));
		const top = () => opts.top ? opts.top() : ((document.querySelector('.site-header') || {}).getBoundingClientRect?.().bottom || 0);
		const tr = (y, s = 1) => `translate(-50%, ${y - 48}px) scale(${s})`;

		const band = dy => 150 * (1 - Math.exp(-dy / 200));

		const pop = e => { if (!reduced()) e.querySelector('.ptr-disc').animate([{ transform: 'scale(1)' }, { transform: 'scale(1.16)' }, { transform: 'scale(1)' }], { duration: 300, easing: SPRING }); };
		function make() {
			el = document.createElement('div'); el.className = 'ptr'; el.setAttribute('aria-hidden', 'true');
			el.innerHTML = '<span class="ptr-disc"><i class="fa-solid fa-rotate-right"></i></span>';
			el.style.top = top() + 'px'; document.body.appendChild(el);
		}
		function draw(y) {
			const p = Math.min(1, y / READY);
			el.style.transform = tr(y, .6 + .4 * Math.min(1, p * 1.5)); el.style.opacity = String(Math.min(1, p * 2.5));
			el.querySelector('i').style.transform = `rotate(${p * 270}deg)`;
		}

		function leave(e, from) {
			if (el === e) el = null;
			if (reduced()) { e.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 150, fill: 'forwards' }).onfinish = () => e.remove(); return; }
			const i = e.querySelector('i');
			e.animate([{ transform: tr(from), opacity: getComputedStyle(e).opacity }, { transform: tr(0, .6), opacity: 0 }], { duration: 320, easing: EASE, fill: 'forwards' }).onfinish = () => e.remove();
			if (!e.classList.contains('done') && !e.classList.contains('fail'))
				i.animate([{ transform: i.style.transform || 'none' }, { transform: 'rotate(0deg)' }], { duration: 320, easing: EASE, fill: 'forwards' });
		}
		addEventListener('touchstart', e => { y0 = e.touches.length === 1 && can(e.target) ? e.touches[0].clientY : null; x0 = y0 == null ? 0 : e.touches[0].clientX; d = 0; }, { passive: true });
		addEventListener('touchmove', e => {
			if (y0 == null) return;
			const dy = e.touches[0].clientY - y0, dx = e.touches[0].clientX - x0;
			if (!el) {
				if (dy > 8 && dy > Math.abs(dx) && scrollY <= 0) { make(); y0 = e.touches[0].clientY; }
				else if (dy < -4 || Math.abs(dx) > 12) y0 = null;
				return;
			}
			d = band(Math.max(0, dy)); draw(d);
			const r = d >= READY;
			if (r !== ready) { ready = r; el.classList.toggle('ready', r); if (r) { pop(el); if (navigator.vibrate) try { navigator.vibrate(8); } catch {} } }
		}, { passive: true });
		const end = async () => {
			if (y0 == null) return; y0 = null; if (!el) return;
			const e = el;
			if (!ready) { leave(e, d); return; }
			busy = true; ready = false; el = null;

			e.style.transform = tr(REST); if (!reduced()) e.animate([{ transform: tr(d) }, { transform: tr(REST) }], { duration: 380, easing: SPRING });
			e.classList.add('busy'); const i = e.querySelector('i'), a = parseFloat(i.style.transform.slice(7)) || 270;
			spin = reduced() ? null : i.animate([{ transform: `rotate(${a}deg)` }, { transform: `rotate(${a + 360}deg)` }], { duration: 700, iterations: Infinity });
			const t0 = Date.now(); let ok = true;
			try { ok = (await (opts.onRefresh && opts.onRefresh())) !== false; } catch { ok = false; }
			await wait(Math.max(0, 600 - (Date.now() - t0)));

			if (spin) { const now = getComputedStyle(i).transform; spin.cancel(); spin = null; await i.animate([{ transform: now, opacity: 1 }, { transform: now + ' scale(.3)', opacity: 0 }], { duration: 140, easing: EASE, fill: 'forwards' }).finished; }
			e.classList.remove('busy', 'ready'); e.classList.add(ok ? 'done' : 'fail');
			const t = document.createElement('template'); t.innerHTML = ok ? '<svg class="k-check" viewBox="0 0 24 24"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>' : '<i class="fa-solid fa-xmark"></i>';
			i.replaceWith(t.content.firstElementChild); pop(e);
			await wait(500); leave(e, REST); busy = false;
		};
		addEventListener('touchend', end);
		addEventListener('touchcancel', () => { if (y0 == null) return; y0 = null; if (el && !busy) leave(el, d); });
	}

	const clock = s => { s = Math.max(0, Math.floor(s || 0)); const h = Math.floor(s / 3600), m = Math.floor(s / 60) % 60, x = String(s % 60).padStart(2, '0'); return h ? `${h}:${String(m).padStart(2, '0')}:${x}` : `${m}:${x}`; };
	const fa = n => `<i class="fa-solid fa-${n}" aria-hidden="true"></i>`;
	function clip(vid) {
		if (vid._k) return; vid._k = true;
		vid.muted = true; vid.loop = true; vid.playsInline = true; vid.removeAttribute('controls'); vid.classList.add('clip');
		if (reduced()) { vid.addEventListener('click', () => vid.paused ? vid.play() : vid.pause()); return; }
		new IntersectionObserver(es => es.forEach(e => { if (e.isIntersecting) vid.play().catch(() => {}); else vid.pause(); }), { threshold: .25 }).observe(vid);
	}
	function player(vid) {
		if (vid._k) return vid.closest('.player'); vid._k = true;
		vid.removeAttribute('controls'); vid.playsInline = true; if (!vid.getAttribute('preload')) vid.preload = 'metadata';
		const p = document.createElement('div'); p.className = 'player fresh'; p.tabIndex = 0; p.setAttribute('aria-label', 'Video player');
		vid.replaceWith(p); p.appendChild(vid);
		p.insertAdjacentHTML('beforeend', `<button type="button" class="pl-big" aria-label="Play">${fa('play')}</button><span class="pl-spin" hidden></span><span class="pl-skip l"></span><span class="pl-skip r"></span>
			<div class="pl-bar"><button type="button" class="pl-btn pl-play" aria-label="Play" data-tip="Play">${fa('play')}</button><span class="pl-time">0:00 / 0:00</span>
			<div class="pl-seek" role="slider" tabindex="0" aria-label="Seek" aria-valuemin="0" aria-valuemax="100" aria-valuenow="0"><span class="pl-track"><span class="pl-buf"></span><span class="pl-fill"></span></span><span class="pl-knob"></span><span class="pl-at" hidden></span></div>
			<div class="pl-vol"><button type="button" class="pl-btn pl-mute" aria-label="Mute" data-tip="Mute">${fa('volume-high')}</button><input class="pl-volume" type="range" min="0" max="1" step="0.05" value="1" aria-label="Volume"></div>
			<button type="button" class="pl-btn pl-speed" aria-label="Speed" data-tip="Speed">1×</button><button type="button" class="pl-btn pl-cc" aria-label="Captions" data-tip="Captions" hidden>${fa('closed-captioning')}</button>
			<button type="button" class="pl-btn pl-pip" aria-label="Picture in picture" data-tip="Picture in picture" hidden>${fa('picture-in-picture')}</button><button type="button" class="pl-btn pl-full" aria-label="Full screen" data-tip="Full screen">${fa('expand')}</button></div>`);
		const $ = s => p.querySelector(s), seek = $('.pl-seek'), at = $('.pl-at');
		let idle = 0, menu = null;
		const setIcon = (b, n) => { b.querySelector('i').className = `fa-solid fa-${n}`; };
		const wake = () => { p.classList.remove('idle'); clearTimeout(idle); if (!vid.paused && !menu) idle = setTimeout(() => p.classList.add('idle'), 2500); };
		const toggle = () => { if (vid.paused || vid.ended) vid.play().catch(() => {}); else vid.pause(); };
		const skip = d => { vid.currentTime = Math.max(0, Math.min(vid.duration || 0, vid.currentTime + d)); wake(); };
		const draw = () => {
			const d = vid.duration || 0, f = d ? vid.currentTime / d : 0;
			p.style.setProperty('--played', f * 100 + '%'); seek.setAttribute('aria-valuenow', Math.round(f * 100)); seek.setAttribute('aria-valuetext', clock(vid.currentTime));
			$('.pl-time').textContent = `${clock(vid.currentTime)} / ${clock(d)}`;
			const b = vid.buffered; p.style.setProperty('--buffered', (d && b.length ? b.end(b.length - 1) / d * 100 : 0) + '%');
		};
		const sound = () => { const off = vid.muted || vid.volume === 0; setIcon($('.pl-mute'), off ? 'volume-xmark' : vid.volume < .5 ? 'volume-low' : 'volume-high'); $('.pl-volume').value = off ? 0 : vid.volume; $('.pl-mute').dataset.tip = off ? 'Unmute' : 'Mute'; };
		vid.addEventListener('play', () => { p.classList.remove('fresh', 'paused'); setIcon($('.pl-play'), 'pause'); $('.pl-play').dataset.tip = 'Pause'; $('.pl-play').setAttribute('aria-label', 'Pause'); wake(); });
		vid.addEventListener('pause', () => { p.classList.add('paused'); setIcon($('.pl-play'), 'play'); $('.pl-play').dataset.tip = 'Play'; $('.pl-play').setAttribute('aria-label', 'Play'); wake(); });
		['timeupdate', 'progress', 'loadedmetadata', 'durationchange'].forEach(t => vid.addEventListener(t, draw));
		vid.addEventListener('volumechange', sound);
		vid.addEventListener('waiting', () => { $('.pl-spin').hidden = false; });
		['playing', 'canplay', 'pause'].forEach(t => vid.addEventListener(t, () => { $('.pl-spin').hidden = true; }));
		vid.addEventListener('loadedmetadata', () => { $('.pl-cc').hidden = !vid.textTracks.length; });
		$('.pl-pip').hidden = !(document.pictureInPictureEnabled && vid.requestPictureInPicture);

		const frac = e => { const r = seek.getBoundingClientRect(); return Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)); };
		seek.addEventListener('pointermove', e => { if (!vid.duration) return; const f = frac(e); at.hidden = false; at.textContent = clock(f * vid.duration); at.style.left = f * 100 + '%'; if (seek.hasPointerCapture(e.pointerId)) vid.currentTime = f * vid.duration; });
		seek.addEventListener('pointerleave', () => { at.hidden = true; });
		seek.addEventListener('pointerdown', e => { if (!vid.duration) return; seek.setPointerCapture(e.pointerId); p.classList.add('seeking'); vid.currentTime = frac(e) * vid.duration; });
		seek.addEventListener('pointerup', () => p.classList.remove('seeking'));
		seek.addEventListener('keydown', e => { if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') { e.preventDefault(); e.stopPropagation(); skip(e.key === 'ArrowLeft' ? -5 : 5); } });
		$('.pl-big').addEventListener('click', toggle);
		$('.pl-play').addEventListener('click', toggle);
		$('.pl-mute').addEventListener('click', () => { vid.muted = !vid.muted; if (!vid.muted && !vid.volume) vid.volume = .5; });
		$('.pl-volume').addEventListener('input', e => { vid.volume = +e.target.value; vid.muted = vid.volume === 0; });
		$('.pl-cc').addEventListener('click', () => { const t = vid.textTracks[0]; if (!t) return; t.mode = t.mode === 'showing' ? 'hidden' : 'showing'; $('.pl-cc').classList.toggle('on', t.mode === 'showing'); });
		$('.pl-pip').addEventListener('click', () => { (document.pictureInPictureElement ? document.exitPictureInPicture() : vid.requestPictureInPicture()).catch(() => {}); });
		const full = () => { const on = document.fullscreenElement === p || document.webkitFullscreenElement === p; if (on) (document.exitFullscreen || document.webkitExitFullscreen).call(document); else (p.requestFullscreen || p.webkitRequestFullscreen || (() => vid.webkitEnterFullscreen && vid.webkitEnterFullscreen())).call(p); };
		$('.pl-full').addEventListener('click', full);
		document.addEventListener('fullscreenchange', () => { const on = document.fullscreenElement === p; setIcon($('.pl-full'), on ? 'compress' : 'expand'); $('.pl-full').dataset.tip = on ? 'Exit full screen' : 'Full screen'; });

		$('.pl-speed').addEventListener('click', e => {
			e.stopPropagation();
			if (menu) { closeMenu(); return; }
			menu = document.createElement('div'); menu.className = 'menu pl-menu'; menu.setAttribute('role', 'menu');
			menu.innerHTML = [.5, 1, 1.25, 1.5, 2].map(r => `<button type="button" class="menu-item" role="menuitemradio" aria-selected="${vid.playbackRate === r}" data-r="${r}">${num(r)}×</button>`).join('');
			p.appendChild(menu); clearTimeout(idle);
			const b = $('.pl-speed').getBoundingClientRect(), q = p.getBoundingClientRect();
			menu.style.right = Math.max(8, q.right - b.right) + 'px'; menu.style.bottom = (q.bottom - b.top + 8) + 'px';
			menu.addEventListener('click', ev => { const it = ev.target.closest('[data-r]'); if (!it) return; vid.playbackRate = +it.dataset.r; $('.pl-speed').textContent = num(+it.dataset.r) + '×'; closeMenu(); });
		});
		function closeMenu() { if (!menu) return; const m = menu; menu = null; m.classList.add('closing'); setTimeout(() => m.remove(), 150); wake(); }
		document.addEventListener('click', e => { if (menu && !menu.contains(e.target)) closeMenu(); });

		let lastTap = 0, tapTimer = 0;
		vid.addEventListener('click', e => {
			if (e.pointerType !== 'touch' && !matchMedia('(pointer: coarse)').matches) { toggle(); return; }
			const r = p.getBoundingClientRect(), x = (e.clientX - r.left) / r.width, side = x < 1 / 3 ? -1 : x > 2 / 3 ? 1 : 0;
			if (side && Date.now() - lastTap < 300) {
				clearTimeout(tapTimer); lastTap = 0; skip(side * 10);
				const pill = $(side < 0 ? '.pl-skip.l' : '.pl-skip.r'); pill.textContent = side < 0 ? '−10 s' : '+10 s'; pill.classList.remove('show'); void pill.offsetWidth; pill.classList.add('show');
				return;
			}
			lastTap = Date.now(); clearTimeout(tapTimer);
			tapTimer = setTimeout(() => { if (p.classList.contains('idle') || vid.paused) { p.classList.remove('idle'); wake(); } else { clearTimeout(idle); p.classList.add('idle'); } }, 300);
		});
		p.addEventListener('pointermove', e => { if (e.pointerType === 'mouse') wake(); });
		p.addEventListener('keydown', e => {
			if (e.target.closest('.pl-menu, .pl-volume')) return;
			const k = e.key.toLowerCase(), act = { ' ': toggle, k: toggle, j: () => skip(-10), l: () => skip(10), arrowleft: () => skip(-5), arrowright: () => skip(5), m: () => $('.pl-mute').click(), f: full, c: () => $('.pl-cc').click() }[k];
			if (!act || (k === ' ' && e.target.closest('button'))) return;
			e.preventDefault(); act(); wake();
		});
		draw(); sound();
		return p;
	}

	function enhance(root = document) {
		root.querySelectorAll('.seg').forEach(initSeg);
		root.querySelectorAll('input.slider').forEach(initSlider);
		root.querySelectorAll('input[type="date"]').forEach(initDate);
		root.querySelectorAll('video.player').forEach(player);
		root.querySelectorAll('video[data-clip]').forEach(clip);
	}

	function roll(el, text, dir = 1) {
		text = String(text);
		const old = el.dataset.val != null ? el.dataset.val : el.textContent;
		el.dataset.val = text;
		if (reduced() || old === text) { el.textContent = text; return; }

		const cs = getComputedStyle(el), lh = parseFloat(cs.lineHeight) || parseFloat(cs.fontSize) * 1.2;
		const n = Math.max(old.length, text.length), o = old.padStart(n, ' '), t = text.padStart(n, ' ');
		const box = document.createElement('span'); box.className = 'roll'; box.style.height = lh + 'px'; box.style.lineHeight = lh + 'px';
		[...t].forEach((ch, i) => {
			const s = document.createElement('span');
			if (ch === o[i]) { s.textContent = ch.trim(); box.appendChild(s); return; }
			const a = document.createElement('span'), b = document.createElement('span');
			a.style.display = b.style.display = 'block';
			a.textContent = (dir > 0 ? o[i] : ch).trim() || '​'; b.textContent = (dir > 0 ? ch : o[i]).trim() || '​';
			s.append(a, b); box.appendChild(s);
			s.animate(dir > 0 ? [{ transform: 'translateY(0)' }, { transform: `translateY(${-lh}px)` }] : [{ transform: `translateY(${-lh}px)` }, { transform: 'translateY(0)' }], { duration: 260, easing: EASE, fill: 'forwards' });
		});
		el.textContent = ''; el.appendChild(box);
		clearTimeout(el._roll); el._roll = setTimeout(() => { el.textContent = text; }, 280);
	}

	function flash(btn, ok = true) {
		if (btn.dataset.flashing) return; btn.dataset.flashing = '1';
		const icon = btn.querySelector('svg, i'), label = [...btn.childNodes].find(n => n.nodeType === 3 && n.textContent.trim()) || btn.querySelector('span');
		const oldIcon = icon ? icon.outerHTML : '', oldText = label ? label.textContent : '';
		const w0 = btn.offsetWidth, swap = el => { if (el && !reduced()) el.animate([{ opacity: 0, filter: 'blur(2px)' }, { opacity: 1, filter: 'none' }], { duration: 220, easing: EASE }); };
		const setIcon = html => { const cur = btn.querySelector('svg, i'); if (!cur) return; const t = document.createElement('template'); t.innerHTML = html; const n = t.content.firstElementChild; cur.replaceWith(n); swap(btn.querySelector('svg, i')); };

		let w1 = w0;
		if (label) {
			btn.style.width = w0 + 'px';
			label.textContent = btn.dataset.done || (ok ? 'Copied' : 'Failed');
			btn.style.width = 'auto'; w1 = btn.offsetWidth; btn.style.width = w0 + 'px'; void btn.offsetWidth;
			btn.style.transition = 'background-color 150ms, color 150ms, width 220ms ' + EASE;
			btn.style.width = w1 + 'px';
		} else btn.style.transition = 'background-color 150ms, color 150ms';
		btn.style.background = ok ? 'var(--success)' : 'var(--danger)'; btn.style.color = '#fff';

		setIcon(ok ? '<svg class="k-check" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>' : '<i class="fa-solid fa-xmark" aria-hidden="true"></i>'); swap(label && label.nodeType === 1 ? label : null);
		setTimeout(() => {
			if (label) { label.textContent = oldText; btn.style.width = 'auto'; const w2 = btn.offsetWidth; btn.style.width = w1 + 'px'; void btn.offsetWidth; btn.style.width = w2 + 'px'; }
			btn.style.background = ''; btn.style.color = '';
			if (oldIcon) setIcon(oldIcon);
			setTimeout(() => { btn.style.width = ''; btn.style.transition = ''; delete btn.dataset.flashing; }, 240);
		}, 1600);
	}

	const ICONS = { info: 'fa-circle-info', ok: 'fa-circle-check', err: 'fa-circle-exclamation', warn: 'fa-triangle-exclamation' };
	function host() {
		let h = document.querySelector('.toast-host');
		if (!h) { h = document.createElement('div'); h.className = 'toast-host'; h.setAttribute('role', 'status'); h.setAttribute('aria-live', 'polite'); document.body.appendChild(h); }
		return h;
	}
	function glide(h, change) {
		const before = new Map([...h.children].map(k => [k, k.getBoundingClientRect().top]));
		change();
		if (reduced()) return;
		for (const k of h.children) { if (!before.has(k)) continue; const dy = before.get(k) - k.getBoundingClientRect().top; if (dy) k.animate([{ transform: `translateY(${dy}px)` }, { transform: 'none' }], { duration: 240, easing: EASE }); }
	}

	function fit(t) { const s = t.querySelector('span'); t.classList.toggle('multi', s.getBoundingClientRect().height > parseFloat(getComputedStyle(s).lineHeight) * 1.5); }
	function toast(message, kind = 'info', opts = {}) {
		const h = host(), live = [...h.querySelectorAll('.toast:not(.leaving)')];
		const life = opts.ms || (opts.action ? 6000 : kind === 'err' ? 5000 : 3000);
		const dup = !opts.action && live.find(t => t.dataset.msg === message && t.dataset.kind === kind);
		if (dup) {
			const n = (+dup.dataset.n || 1) + 1; dup.dataset.n = n;
			let c = dup.querySelector('.toast-count'); if (!c) { c = document.createElement('span'); c.className = 'toast-count'; dup.appendChild(c); }
			c.textContent = '×' + n; fit(dup);
			if (!reduced()) dup.animate([{ transform: 'scale(1)' }, { transform: 'scale(1.04)' }, { transform: 'scale(1)' }], { duration: 220, easing: SPRING });
			dup._restart(); return dup;
		}
		const t = document.createElement('div'); t.className = 'toast' + (kind === 'info' ? '' : ' ' + kind); t.dataset.msg = message; t.dataset.kind = kind;
		t.innerHTML = `<i class="fa-solid ${opts.icon || ICONS[kind] || ICONS.info}" aria-hidden="true"></i><span></span>`;
		t.querySelector('span').textContent = message;
		if (opts.action) {
			const b = document.createElement('button'); b.type = 'button'; b.className = 'toast-act'; b.textContent = opts.action;
			b.addEventListener('click', e => { e.stopPropagation(); leave(); opts.onAction && opts.onAction(); });
			t.appendChild(b);
		}
		let timer = 0, left = life, since = 0;
		const run = () => { since = Date.now(); timer = setTimeout(leave, left); };
		function leave() { if (t.classList.contains('leaving')) return; clearTimeout(timer); t.classList.add('leaving'); setTimeout(() => glide(h, () => t.remove()), 200); }
		t._restart = () => { clearTimeout(timer); left = life; run(); };
		t.addEventListener('mouseenter', () => { clearTimeout(timer); left -= Date.now() - since; });
		t.addEventListener('mouseleave', run);
		t.addEventListener('click', leave);
		glide(h, () => { h.prepend(t); fit(t); });
		[...h.querySelectorAll('.toast:not(.leaving)')].slice(3).forEach(o => { o.classList.add('leaving'); setTimeout(() => o.remove(), 200); });
		run();
		return t;
	}

	function num(n, digits) {
		const o = digits == null ? { maximumFractionDigits: 2 } : { minimumFractionDigits: digits, maximumFractionDigits: digits };
		return new Intl.NumberFormat('id-ID', o).format(n);
	}
	function date(d, style = 'long') {
		d = d instanceof Date ? d : new Date(d);
		if (style === 'numeric') return [d.getDate(), d.getMonth() + 1].map(x => String(x).padStart(2, '0')).join('/') + '/' + d.getFullYear();
		if (style === 'time') return String(d.getHours()).padStart(2, '0') + ':' + String(d.getMinutes()).padStart(2, '0');
		return new Intl.DateTimeFormat('en-GB', { day: 'numeric', month: style === 'short' ? 'short' : 'long', year: 'numeric' }).format(d);
	}

	(function sheets() {
		const phone = matchMedia('(max-width: 600px)');
		const SKIP = 'input, select, textarea, [contenteditable], canvas, [data-no-drag]';
		let m = null, from = null, head = false, x0 = 0, y0 = 0, dy = 0, t0 = 0, mode = 0, swallow = 0;
		const closer = el => el.querySelector('.modal-head [data-close-modal], .modal-head [data-close-confirm], .modal-head [aria-label="Close"]');
		const blocked = () => { for (let n = from; n && n !== m; n = n.parentElement) if (n.matches(SKIP) || (!head && getComputedStyle(n).touchAction === 'none')) return true; return false; };
		const scrolled = () => { for (let n = from; n && n !== m; n = n.parentElement) if (n.scrollTop > 0) return true; return false; };
		document.addEventListener('touchstart', e => {
			m = null; mode = -1;
			if (!phone.matches || e.touches.length !== 1) return;
			from = e.target; m = from.closest && from.closest('.modal.open'); if (!m) return;
			head = !!from.closest('.modal-head'); x0 = e.touches[0].clientX; y0 = e.touches[0].clientY; dy = 0; t0 = Date.now();
			mode = blocked() ? -1 : 0;
		}, { passive: true });
		document.addEventListener('touchmove', e => {
			if (!m || mode < 0) return;
			const t = e.touches[0], down = t.clientY - y0;
			if (!mode) {
				if (Math.abs(down) + Math.abs(t.clientX - x0) < 8) return;
				if (down > 0 && down > Math.abs(t.clientX - x0) && !scrolled()) { mode = 1; m.style.transition = 'none'; } else { mode = -1; return; }
			}
			dy = Math.max(0, down);
			m.style.translate = '0 ' + dy + 'px';
			if (e.cancelable) e.preventDefault();
		}, { passive: false });
		const end = () => {
			if (!m || mode !== 1) { m = null; return; }
			const el = m, fast = dy / Math.max(1, Date.now() - t0) > 0.5, gone = dy > 90 || (dy > 30 && fast), btn = closer(el);
			m = null; mode = -1;
			el.style.transition = 'translate 200ms ease';
			if (gone && btn) {
				el.style.translate = '0 ' + el.offsetHeight + 'px';
				btn.click();
				swallow = Date.now() + 400;
				setTimeout(() => { el.style.transition = ''; el.style.translate = ''; }, 320);
			} else {
				el.style.translate = '';
				swallow = Date.now() + 400;
				setTimeout(() => { el.style.transition = ''; }, 220);
			}
		};
		document.addEventListener('touchend', end);
		document.addEventListener('touchcancel', end);
		document.addEventListener('click', e => { if (Date.now() < swallow) { e.preventDefault(); e.stopPropagation(); } }, true);
	})();

	window.Krysion = { toast, flash, roll, enhance, num, date, reorder, viewer, pull, player };

	const WATCH = '.seg, input.slider, input[type="date"], video.player, video[data-clip]';
	let queued = 0;
	const watch = () => new MutationObserver(muts => {
		if (queued) return;
		for (const m of muts) for (const n of m.addedNodes) if (n.nodeType === 1 && (n.matches(WATCH) || n.querySelector(WATCH))) { queued = setTimeout(() => { queued = 0; enhance(); }); return; }
	}).observe(document.body, { childList: true, subtree: true });
	if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', () => { enhance(); watch(); }); else { enhance(); watch(); }
})();
