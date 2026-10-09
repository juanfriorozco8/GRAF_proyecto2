// @ts-check
(() => {
  const view = document.querySelector('#view');
  const status = document.querySelector('#status');
  if (!(view instanceof HTMLImageElement) || !(status instanceof HTMLElement)) return;

  const movementKeys = new Set(['a', 'd', 'w', 's', 'r', 'f', 'arrowleft', 'arrowright', 'arrowup', 'arrowdown']);
  /** @type {Set<string>} */
  const pressed = new Set();
  let angle = -1.42, radius = 130, elevation = 100;
  let busy = false, settleTimer = 0, motionEpoch = 0, resetEpoch = 0;
  /** @type {'fast' | 'high' | null} */
  let pending = null;
  /** @type {string | null} */
  let displayedUrl = null;

  /** @param {'fast' | 'high'} quality */
  function requestDraw(quality) {
    pending = quality;
    void pump();
  }

  async function pump() {
    if (busy || pending === null) return;
    const quality = pressed.size > 0 ? 'fast' : pending;
    const requestedMotion = motionEpoch;
    const requestedReset = resetEpoch;
    pending = null;
    busy = true;
    /** @type {string | null} */
    let nextUrl = null;
    try {
      const response = await fetch(`/render?angle=${angle}&radius=${radius}&elevation=${elevation}&quality=${quality}`, { cache: 'no-store' });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      nextUrl = URL.createObjectURL(await response.blob());
      // Decode separately while the previous frame stays on screen. No blending,
      // reprojection or CSS transition is applied to the scene.
      const nextFrame = new Image();
      nextFrame.src = nextUrl;
      await nextFrame.decode();
      const current = requestedReset === resetEpoch &&
        (quality === 'fast' || (pressed.size === 0 && requestedMotion === motionEpoch));
      if (current) {
        view.src = nextUrl;
        if (displayedUrl !== null) URL.revokeObjectURL(displayedUrl);
        displayedUrl = nextUrl;
        nextUrl = null;
        status.textContent = '';
        const samples = Number(response.headers.get('X-Render-Samples'));
        const target = Number(response.headers.get('X-Target-Samples'));
        if (quality === 'high' && samples > 0 && target > samples && pending === null) pending = 'high';
      }
    } catch (error) {
      status.textContent = error instanceof Error ? `Error: ${error.message}` : 'No se pudo actualizar la vista';
    } finally {
      if (nextUrl !== null) URL.revokeObjectURL(nextUrl);
      busy = false;
      if (pending !== null) void pump();
    }
  }

  function refineAfterMovement() {
    clearTimeout(settleTimer);
    settleTimer = setTimeout(() => {
      if (pressed.size === 0) requestDraw('high');
    }, 140);
  }

  addEventListener('keydown', event => {
    const key = event.key.toLowerCase();
    if (movementKeys.has(key)) {
      if (!pressed.has(key)) motionEpoch += 1;
      pressed.add(key);
      clearTimeout(settleTimer);
      event.preventDefault();
    } else if (key === '0') {
      angle = -1.42; radius = 130; elevation = 100;
      resetEpoch += 1;
      motionEpoch += 1;
      pressed.clear();
      clearTimeout(settleTimer);
      requestDraw('fast');
      refineAfterMovement();
      event.preventDefault();
    }
  });
  addEventListener('keyup', event => {
    if (pressed.delete(event.key.toLowerCase()) && pressed.size === 0) refineAfterMovement();
  });
  addEventListener('blur', () => {
    pressed.clear();
    motionEpoch += 1;
    refineAfterMovement();
  });

  let previousTime = performance.now();
  /** @param {number} now */
  function animate(now) {
    const seconds = Math.min((now - previousTime) / 1000, 0.05);
    previousTime = now;
    const oldAngle = angle, oldRadius = radius, oldElevation = elevation;
    if (pressed.has('a') || pressed.has('arrowleft')) angle -= 1.15 * seconds;
    if (pressed.has('d') || pressed.has('arrowright')) angle += 1.15 * seconds;
    if (pressed.has('w') || pressed.has('arrowup')) radius = Math.max(85, radius - 35 * seconds);
    if (pressed.has('s') || pressed.has('arrowdown')) radius = Math.min(210, radius + 35 * seconds);
    if (pressed.has('r')) elevation = Math.min(155, elevation + 28 * seconds);
    if (pressed.has('f')) elevation = Math.max(55, elevation - 28 * seconds);
    if (angle !== oldAngle || radius !== oldRadius || elevation !== oldElevation) requestDraw('fast');
    requestAnimationFrame(animate);
  }
  requestAnimationFrame(animate);
  requestDraw('high');
})();
