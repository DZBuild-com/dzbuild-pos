let context: AudioContext | null = null;

/** Scan OK / error cues (SAL-036, UX-034); silent where Web Audio is missing (tests, locked audio). */
export function beep(kind: "ok" | "error"): void {
  if (typeof window.AudioContext === "undefined") return;
  try {
    context ??= new AudioContext();
    const oscillator = context.createOscillator();
    const gain = context.createGain();
    oscillator.frequency.value = kind === "ok" ? 1_760 : 220;
    gain.gain.value = 0.08;
    oscillator.connect(gain).connect(context.destination);
    oscillator.start();
    oscillator.stop(context.currentTime + (kind === "ok" ? 0.08 : 0.3));
  } catch {
    // no audio device: the screen still shows the result
  }
}
