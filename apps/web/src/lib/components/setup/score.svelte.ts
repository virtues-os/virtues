/**
 * Setup's sound: the track under Welcome and the letter, and the three
 * notes the ∴ lands on.
 *
 * THE TRACK is "Tears and Fireflies" (Adi Goldstein, licensed through
 * AGsoundtrax to Adam, ref AGX-2026-00084), about 80s. It starts on Welcome
 * and carries on under the letter, so the cold open and the letter are one
 * scene, and it fades out when the letter's button is pressed (or anyone
 * leaves the two by a dot). It lives here, not in Hello, because Hello is
 * gone by the time the letter is read.
 *
 * THE THUMP. One low, soft thud as each dot of the mark lands, like a
 * heartbeat heard through a wall: premise, premise, therefore, the third
 * heavier with a longer tail. Foley, not music: a pitched note (the first
 * try, a marimba-like E-G-C) read as a jingle, and a pen tap and a water
 * drop lost to this on audition (2026-09-25). Synthesized, so there is no
 * file and no licence.
 *
 * A browser starts sound only after a gesture, so both try at once and stay
 * silent when refused; the app's webview allows it without one (wry's
 * `autoplay` is on by default), which is where a first run happens. A
 * missing file is silence. Reduced motion gets no notes.
 */

const TRACK = '/onboarding/hello.mp3';
const VOLUME = 0.8;

class Score {
	playing = $state(false);
	muted = $state(false);

	#audio: HTMLAudioElement | null = null;
	#ctx: AudioContext | null = null;
	#fading = false;

	/** Load the track and try to start it; again on each gesture until it plays. */
	start() {
		if (typeof window === 'undefined') return;
		if (!this.#audio) {
			const a = new Audio(TRACK);
			a.preload = 'auto';
			a.addEventListener('error', () => (this.#audio = null), { once: true });
			this.#audio = a;
		}
		this.#play();
	}

	#play() {
		const a = this.#audio;
		if (!a || this.playing || this.muted || this.#fading) return;
		a.volume = VOLUME;
		a.play()
			.then(() => (this.playing = true))
			.catch(() => {
				/* blocked until a gesture, or no file: both are fine */
			});
	}

	/** The track goes quiet over `ms`, then stops for good. */
	fade(ms = 1200) {
		const a = this.#audio;
		if (!a) return;
		this.#fading = true;
		if (a.paused || ms <= 0) {
			this.#stop();
			return;
		}
		const from = a.volume;
		const t0 = performance.now();
		const step = (t: number) => {
			const k = Math.min(1, (t - t0) / ms);
			a.volume = from * (1 - k);
			if (k < 1) requestAnimationFrame(step);
			else this.#stop();
		};
		requestAnimationFrame(step);
	}

	#stop() {
		this.#audio?.pause();
		this.#audio = null;
		this.playing = false;
		this.#fading = false;
	}

	toggleMute() {
		this.muted = !this.muted;
		if (this.#audio) this.#audio.muted = this.muted;
		if (!this.muted) this.#play();
	}

	/** The thump for dot `i` of the mark (0, 1, 2). */
	note(i: number) {
		if (this.muted || typeof window === 'undefined') return;
		if (window.matchMedia?.('(prefers-reduced-motion: reduce)').matches) return;
		try {
			this.#ctx ??= new AudioContext();
			const ctx = this.#ctx;
			if (ctx.state === 'suspended') void ctx.resume();
			if (ctx.state === 'closed') return;
			const t = ctx.currentTime + 0.01;
			const apex = i === 2;
			// A sine falling from 80 to 44Hz, with low-passed noise for the body.
			this.#tone(ctx, t, { from: 80, to: 44, glide: 0.12, level: apex ? 0.6 : 0.45, decay: apex ? 0.5 : 0.28 });
			this.#noise(ctx, t, { type: 'lowpass', freq: 380, q: 0.5, level: apex ? 0.14 : 0.1, decay: apex ? 0.2 : 0.12 });
		} catch {
			/* no Web Audio: silence */
		}
	}

	#tone(ctx: AudioContext, t: number, o: { from: number; to: number; glide: number; level: number; decay: number }) {
		const osc = ctx.createOscillator();
		const env = ctx.createGain();
		osc.type = 'sine';
		osc.frequency.setValueAtTime(o.from, t);
		osc.frequency.exponentialRampToValueAtTime(o.to, t + o.glide);
		env.gain.setValueAtTime(0, t);
		env.gain.linearRampToValueAtTime(o.level, t + 0.004);
		env.gain.exponentialRampToValueAtTime(0.0001, t + o.decay);
		osc.connect(env).connect(ctx.destination);
		osc.start(t);
		osc.stop(t + o.decay + 0.05);
	}

	#noise(ctx: AudioContext, t: number, o: { type: BiquadFilterType; freq: number; q: number; level: number; decay: number }) {
		const len = Math.ceil(ctx.sampleRate * (o.decay + 0.05));
		const buf = ctx.createBuffer(1, len, ctx.sampleRate);
		const data = buf.getChannelData(0);
		for (let k = 0; k < len; k++) data[k] = Math.random() * 2 - 1;
		const src = ctx.createBufferSource();
		src.buffer = buf;
		const filter = ctx.createBiquadFilter();
		filter.type = o.type;
		filter.frequency.value = o.freq;
		filter.Q.value = o.q;
		const env = ctx.createGain();
		env.gain.setValueAtTime(0, t);
		env.gain.linearRampToValueAtTime(o.level, t + 0.002);
		env.gain.exponentialRampToValueAtTime(0.0001, t + o.decay);
		src.connect(filter).connect(env).connect(ctx.destination);
		src.start(t);
	}
}

export const score = new Score();
