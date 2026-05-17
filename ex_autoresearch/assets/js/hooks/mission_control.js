// Mission Control — Three.js-driven 3D LiveView hook.
//
// Loads three.js + addons from CDN on first mount (matches the Mermaid hook
// pattern in app.js). Keeps the app.js bundle slim — three.js (~600KB) is
// only fetched when the user actually visits /mission.
//
// Server → client events (LiveView push_event):
//   "mc:agent"    %{status, step, progress, report_id}
//   "mc:reactor"  %{input_tokens, output_tokens, calls, budget}
//   "mc:scraper"  %{url, outcome, duration_ms, fallback_used?, message}
//   "mc:alert"    %{type, message}
//   "mc:reset"    %{}
//
// Client → server events (this.pushEvent):
//   "intent:start_research"  %{query, model, max_depth, max_sources}
//   "intent:stop_research"   %{}
//   "intent:focus_satellite" %{id}

const THREE_VERSION = "0.160.0"
const CDN = `https://esm.sh/three@${THREE_VERSION}`

const STATUS_COLORS = {
  idle:        0x4b5563, // gray
  planning:    0x3b82f6, // blue
  searching:   0x06b6d4, // cyan
  analyzing:   0xa855f7, // purple
  deep_dive:   0xec4899, // pink
  writing:     0xf59e0b, // gold
  completed:   0x10b981, // green
  failed:      0xef4444, // red
  researching: 0x06b6d4, // alias
}

const SCRAPER_COLORS = {
  primary_success:  0x10b981, // green
  fallback_success: 0xf59e0b, // amber
  both_failed:      0xef4444, // red
  native_failed:    0xef4444,
  default:          0xffffff,
}

const MissionControl = {
  async mounted() {
    this.disposed = false
    this.statusEl = document.querySelector("[data-mc-status]")
    this.stepEl = document.querySelector("[data-mc-step]")
    this.tokensEl = document.querySelector("[data-mc-tokens]")
    this.satCountEl = document.querySelector("[data-mc-sat-count]")
    this.alertsEl = document.querySelector("[data-mc-alerts]")

    try {
      this.three = await import(CDN)
      const [{ GLTFLoader }, { OrbitControls }, { EffectComposer }, { RenderPass }, { UnrealBloomPass }] = await Promise.all([
        import(`${CDN}/examples/jsm/loaders/GLTFLoader.js`),
        import(`${CDN}/examples/jsm/controls/OrbitControls.js`),
        import(`${CDN}/examples/jsm/postprocessing/EffectComposer.js`),
        import(`${CDN}/examples/jsm/postprocessing/RenderPass.js`),
        import(`${CDN}/examples/jsm/postprocessing/UnrealBloomPass.js`),
      ])
      this.GLTFLoader = GLTFLoader
      this.OrbitControls = OrbitControls
      this.EffectComposer = EffectComposer
      this.RenderPass = RenderPass
      this.UnrealBloomPass = UnrealBloomPass
    } catch (e) {
      this.el.innerHTML = `<div class="absolute inset-0 grid place-items-center text-error text-sm">
        Failed to load 3D engine: ${e.message}<br/>Check your network connection to esm.sh.
      </div>`
      return
    }

    if (this.disposed) return
    this.initScene()
    this.bindLiveEvents()
    this.animate()
  },

  destroyed() {
    this.disposed = true
    if (this.bubbleTimer) clearTimeout(this.bubbleTimer)
    if (this.renderer) this.renderer.dispose()
    if (this.ro) this.ro.disconnect()
    if (this.composer) this.composer.dispose && this.composer.dispose()
  },

  // ── Scene setup ─────────────────────────────────────────────────────────

  initScene() {
    const THREE = this.three
    const w = this.el.clientWidth || 800
    const h = this.el.clientHeight || 600

    this.scene = new THREE.Scene()
    this.scene.background = new THREE.Color(0x05070d)
    this.scene.fog = new THREE.FogExp2(0x05070d, 0.018)

    this.camera = new THREE.PerspectiveCamera(48, w / h, 0.1, 200)
    this.camera.position.set(0, 4.5, 13)

    this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: false })
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2))
    this.renderer.setSize(w, h)
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping
    this.renderer.toneMappingExposure = 1.1
    this.el.appendChild(this.renderer.domElement)

    this.controls = new this.OrbitControls(this.camera, this.renderer.domElement)
    this.controls.enableDamping = true
    this.controls.dampingFactor = 0.06
    this.controls.minDistance = 5
    this.controls.maxDistance = 30
    this.controls.target.set(0, 0, 0)
    this.controls.autoRotate = true
    this.controls.autoRotateSpeed = 0.35

    // Lights
    this.scene.add(new THREE.HemisphereLight(0x8aa9ff, 0x0a0a1a, 0.45))
    const key = new THREE.DirectionalLight(0xffffff, 0.8)
    key.position.set(6, 8, 4)
    this.scene.add(key)
    const rim = new THREE.DirectionalLight(0x7dd3fc, 0.6)
    rim.position.set(-7, 2, -5)
    this.scene.add(rim)

    // Starfield
    this.scene.add(this.buildStars(900, 80))

    // Orbit ring (subtle guide)
    const ringGeo = new THREE.RingGeometry(3.6, 3.62, 96)
    const ringMat = new THREE.MeshBasicMaterial({
      color: 0x60a5fa, transparent: true, opacity: 0.18, side: THREE.DoubleSide,
    })
    const ring = new THREE.Mesh(ringGeo, ringMat)
    ring.rotation.x = Math.PI / 2
    this.scene.add(ring)

    // Containers
    this.orchestratorPivot = new THREE.Group()
    this.scene.add(this.orchestratorPivot)
    this.satelliteRoot = new THREE.Group()
    this.scene.add(this.satelliteRoot)

    // Build initial procedural meshes; .glb loads override if available
    this.buildProceduralOrchestrator()
    this.buildReactor()
    this.loadGltfAssets()

    // Post-processing — bloom for the glow
    this.composer = new this.EffectComposer(this.renderer)
    this.composer.addPass(new this.RenderPass(this.scene, this.camera))
    const bloom = new this.UnrealBloomPass(new THREE.Vector2(w, h), 0.85, 0.55, 0.1)
    this.composer.addPass(bloom)

    // Speech bubble overlay — anchored above the orchestrator
    this.buildBubble()

    // Reusable vector for projecting world → screen each frame
    this._projVec = new THREE.Vector3()

    // Raycaster for clicking satellites
    this._raycaster = new THREE.Raycaster()
    this._mouse = new THREE.Vector2()
    this.renderer.domElement.addEventListener("pointerdown", (e) => this.onCanvasPick(e))

    // Resize
    this.ro = new ResizeObserver(() => this.handleResize())
    this.ro.observe(this.el)

    // State
    this.satellites = []
    this.satIdSeed = 0
    this.agentState = { status: "idle", progress: 0 }
    this.reactorState = { input_tokens: 0, output_tokens: 0, budget: 200_000 }
    this.alerts = []
    this.applyAgentColor("idle")
  },

  buildStars(count, range) {
    const THREE = this.three
    const geo = new THREE.BufferGeometry()
    const positions = new Float32Array(count * 3)
    for (let i = 0; i < count; i++) {
      positions[i * 3]     = (Math.random() - 0.5) * range
      positions[i * 3 + 1] = (Math.random() - 0.5) * range
      positions[i * 3 + 2] = (Math.random() - 0.5) * range
    }
    geo.setAttribute("position", new THREE.BufferAttribute(positions, 3))
    const mat = new THREE.PointsMaterial({
      color: 0xcfd8ff, size: 0.06, sizeAttenuation: true, transparent: true, opacity: 0.7,
    })
    return new THREE.Points(geo, mat)
  },

  buildProceduralOrchestrator() {
    const THREE = this.three
    const geo = new THREE.IcosahedronGeometry(1.0, 1)
    const mat = new THREE.MeshStandardMaterial({
      color: 0x4b5563,
      metalness: 0.45,
      roughness: 0.18,
      emissive: 0x000000,
      emissiveIntensity: 1.0,
    })
    const mesh = new THREE.Mesh(geo, mat)
    mesh.name = "orchestrator"
    this.orchestrator = mesh
    this.orchestratorPivot.add(mesh)
  },

  buildReactor() {
    const THREE = this.three
    const group = new THREE.Group()
    group.position.set(5.5, 0, 0)
    // Outer shell
    const outerGeo = new THREE.CylinderGeometry(0.55, 0.55, 2.4, 36, 1, true)
    const outerMat = new THREE.MeshStandardMaterial({
      color: 0x1f2937, metalness: 0.85, roughness: 0.25,
      transparent: true, opacity: 0.35, side: THREE.DoubleSide,
    })
    group.add(new THREE.Mesh(outerGeo, outerMat))
    // Inner core (height-scaled by token fill)
    const innerGeo = new THREE.CylinderGeometry(0.38, 0.38, 2.3, 24)
    const innerMat = new THREE.MeshStandardMaterial({
      color: 0xfb923c, emissive: 0xf97316, emissiveIntensity: 2.0,
      metalness: 0.0, roughness: 0.9,
    })
    const inner = new THREE.Mesh(innerGeo, innerMat)
    inner.position.y = -1.15  // pin to bottom; scaled from origin via group
    inner.geometry.translate(0, 1.15, 0)
    inner.scale.y = 0.02       // start near-empty
    group.add(inner)
    this.reactorInner = inner

    // Caps
    const capGeo = new THREE.TorusGeometry(0.55, 0.04, 8, 36)
    const capMat = new THREE.MeshStandardMaterial({ color: 0x60a5fa, metalness: 0.9, roughness: 0.2 })
    const top = new THREE.Mesh(capGeo, capMat)
    top.position.y = 1.2; top.rotation.x = Math.PI / 2
    const bot = new THREE.Mesh(capGeo, capMat)
    bot.position.y = -1.2; bot.rotation.x = Math.PI / 2
    group.add(top); group.add(bot)

    this.reactor = group
    this.scene.add(group)
  },

  loadGltfAssets() {
    const base = "/3d"
    const loader = new this.GLTFLoader()
    // Orchestrator override — swap mesh geometry, keep our material so we can recolor live.
    loader.load(`${base}/orchestrator.glb`, (g) => {
      const src = this.firstMesh(g.scene)
      if (!src || !this.orchestrator) return
      this.orchestrator.geometry.dispose()
      this.orchestrator.geometry = src.geometry.clone()
      this.applyAgentColor(this.agentState.status)
    }, undefined, () => {/* silent — fallback geometry stays */})

    // Satellite template — store geometry for cloning when events arrive.
    loader.load(`${base}/satellite.glb`, (g) => {
      const src = this.firstMesh(g.scene)
      if (src) this.satelliteGeo = src.geometry.clone()
    }, undefined, () => {})
  },

  firstMesh(root) {
    let found = null
    root.traverse((o) => { if (!found && o.isMesh) found = o })
    return found
  },

  // ── Speech bubble ───────────────────────────────────────────────────────

  buildBubble() {
    const bubble = document.createElement("div")
    bubble.className = [
      "absolute z-20 hidden pointer-events-auto",
      "w-64 -translate-x-1/2 -translate-y-full",
      "rounded-xl border border-amber-400/40 bg-black/80 backdrop-blur-md",
      "px-3 py-2.5 text-white shadow-2xl shadow-amber-500/20",
    ].join(" ")
    bubble.innerHTML = `
      <div class="flex items-start gap-2">
        <div class="mt-0.5 h-2 w-2 shrink-0 rounded-full bg-amber-400 animate-pulse"></div>
        <div class="min-w-0 flex-1">
          <div data-bubble-title class="text-[10px] uppercase tracking-[0.18em] text-amber-300/90"></div>
          <div data-bubble-msg class="mt-0.5 text-xs leading-snug text-white/90"></div>
          <div class="mt-2 flex gap-1.5">
            <button data-bubble-stop type="button"
              class="rounded-md bg-red-500/20 border border-red-500/40 px-2 py-1 text-[10px] font-semibold uppercase tracking-wider text-red-200 hover:bg-red-500/30 transition">
              Stop now
            </button>
            <button data-bubble-dismiss type="button"
              class="rounded-md bg-white/10 border border-white/15 px-2 py-1 text-[10px] font-semibold uppercase tracking-wider text-white/80 hover:bg-white/20 transition">
              Got it
            </button>
          </div>
        </div>
      </div>
      <div class="absolute left-1/2 -bottom-1.5 -translate-x-1/2 h-3 w-3 rotate-45 bg-black/80 border-r border-b border-amber-400/40"></div>
    `
    this.el.appendChild(bubble)
    this.bubbleEl = bubble
    this.bubbleTitleEl = bubble.querySelector("[data-bubble-title]")
    this.bubbleMsgEl = bubble.querySelector("[data-bubble-msg]")

    bubble.querySelector("[data-bubble-stop]").addEventListener("click", () => {
      this.pushEvent("intent:stop_research", {})
      this.hideBubble()
    })
    bubble.querySelector("[data-bubble-dismiss]").addEventListener("click", () => {
      this.hideBubble()
    })
  },

  showBubble(title, message) {
    if (!this.bubbleEl) return
    this.bubbleTitleEl.textContent = title
    this.bubbleMsgEl.textContent = message
    this.bubbleEl.classList.remove("hidden")
    if (this.bubbleTimer) clearTimeout(this.bubbleTimer)
    this.bubbleTimer = setTimeout(() => this.hideBubble(), 15_000)
  },

  hideBubble() {
    if (!this.bubbleEl) return
    this.bubbleEl.classList.add("hidden")
    if (this.bubbleTimer) { clearTimeout(this.bubbleTimer); this.bubbleTimer = null }
  },

  updateBubblePosition() {
    if (!this.bubbleEl || this.bubbleEl.classList.contains("hidden")) return
    if (!this.orchestrator) return
    // Anchor a hair above the orchestrator (y=1.4 in world space).
    this._projVec.set(0, 1.4, 0).project(this.camera)
    const w = this.el.clientWidth, h = this.el.clientHeight
    if (this._projVec.z > 1) {
      // Behind the camera — keep bubble visible by clamping to top edge.
      this.bubbleEl.style.left = `${w / 2}px`
      this.bubbleEl.style.top = `40px`
      return
    }
    const x = (this._projVec.x * 0.5 + 0.5) * w
    const y = (-this._projVec.y * 0.5 + 0.5) * h
    this.bubbleEl.style.left = `${x}px`
    this.bubbleEl.style.top  = `${y - 16}px`  // sit a bit above the anchor
  },

  // ── LiveView event wiring ───────────────────────────────────────────────

  bindLiveEvents() {
    this.handleEvent("mc:agent",   (p) => this.onAgent(p))
    this.handleEvent("mc:reactor", (p) => this.onReactor(p))
    this.handleEvent("mc:scraper", (p) => this.onScraper(p))
    this.handleEvent("mc:alert",   (p) => this.onAlert(p))
    this.handleEvent("mc:reset",   ()  => this.onReset())
    this.handleEvent("mc:focus",   (p) => this.onFocus(p))
  },

  onFocus(p) {
    if (this.controls) this.controls.autoRotate = !p.active
  },

  onAgent(p) {
    this.agentState = { ...this.agentState, ...p }
    this.applyAgentColor(p.status)
    if (this.statusEl) this.statusEl.textContent = (p.status || "idle").toUpperCase()
    if (this.stepEl)   this.stepEl.textContent   = p.step || ""
    if (p.status === "completed") {
      this.pulseOrchestrator(1.5)
    }
    if (p.status === "idle" || p.status === "completed" || p.status === "failed") {
      this.hideBubble()
    }
  },

  onReactor(p) {
    this.reactorState = { ...this.reactorState, ...p }
    const total = (p.input_tokens || 0) + (p.output_tokens || 0)
    const budget = Math.max(1, p.budget || this.reactorState.budget)
    const fill = Math.max(0.02, Math.min(1, total / budget))
    if (this.reactorInner) {
      // animate scale.y via simple lerp in animate() — store target
      this.reactorTargetY = fill
    }
    if (this.tokensEl) {
      this.tokensEl.textContent = `${total.toLocaleString()} / ${budget.toLocaleString()} tokens · ${(p.calls || 0)} calls`
    }
  },

  onScraper(p) {
    // Add a satellite that orbits, then settles into its outcome color.
    const THREE = this.three
    const geo = this.satelliteGeo || new THREE.IcosahedronGeometry(0.15, 0)
    const color = SCRAPER_COLORS[p.outcome] ?? SCRAPER_COLORS.default
    const mat = new THREE.MeshStandardMaterial({
      color, emissive: color, emissiveIntensity: 1.4, roughness: 0.5,
    })
    const sat = new THREE.Mesh(geo, mat)
    sat.userData = {
      id: ++this.satIdSeed,
      url: p.url,
      outcome: p.outcome,
      bornAt: performance.now(),
      angle: Math.random() * Math.PI * 2,
      angularVel: 0.35 + Math.random() * 0.25,
      radius: 3.4 + Math.random() * 0.4,
      tiltY: (Math.random() - 0.5) * 0.6,
      ttl: 18_000, // ms before fade
    }
    this.satelliteRoot.add(sat)
    this.satellites.push(sat)

    // Cap satellite count — drop oldest
    while (this.satellites.length > 32) {
      const oldest = this.satellites.shift()
      this.satelliteRoot.remove(oldest)
      oldest.geometry !== this.satelliteGeo && oldest.geometry.dispose()
      oldest.material.dispose()
    }

    if (this.satCountEl) this.satCountEl.textContent = String(this.satellites.length)
  },

  onAlert(p) {
    this.alerts.unshift({ ...p, at: Date.now() })
    this.alerts = this.alerts.slice(0, 5)
    if (this.alertsEl) {
      this.alertsEl.innerHTML = this.alerts.map(a =>
        `<div class="text-xs text-amber-300/90"><span class="opacity-50">[${new Date(a.at).toLocaleTimeString()}]</span> ${a.message || a.type}</div>`
      ).join("")
    }
    const title = (p.type || "alert").replace(/_/g, " ")
    this.showBubble(title, p.message || p.type || "Quality alert")
  },

  onCanvasPick(e) {
    if (!this.satellites || !this.satellites.length) return
    const rect = this.renderer.domElement.getBoundingClientRect()
    this._mouse.x =  ((e.clientX - rect.left) / rect.width)  * 2 - 1
    this._mouse.y = -((e.clientY - rect.top)  / rect.height) * 2 + 1
    this._raycaster.setFromCamera(this._mouse, this.camera)
    const hits = this._raycaster.intersectObjects(this.satellites, false)
    if (!hits.length) return
    const sat = hits[0].object
    const u = sat.userData || {}
    if (!u.url) return
    // Tactile feedback: bump the picked satellite outward briefly.
    sat.scale.setScalar(1.6)
    setTimeout(() => sat.scale.setScalar(1.0), 250)
    this.pushEvent("intent:focus_satellite", {
      url: u.url,
      outcome: u.outcome || "unknown",
    })
  },

  onReset() {
    for (const s of this.satellites) {
      this.satelliteRoot.remove(s)
      s.material.dispose()
    }
    this.satellites = []
    if (this.satCountEl) this.satCountEl.textContent = "0"
    this.reactorTargetY = 0.02
    this.hideBubble()
    this.alerts = []
    if (this.alertsEl) this.alertsEl.innerHTML = ""
  },

  applyAgentColor(status) {
    if (!this.orchestrator) return
    const color = STATUS_COLORS[status] ?? STATUS_COLORS.idle
    this.orchestrator.material.color.setHex(color)
    this.orchestrator.material.emissive.setHex(color)
    this.orchestrator.material.emissiveIntensity =
      status === "idle" ? 0.25 : (status === "completed" ? 1.6 : 1.0)
  },

  pulseOrchestrator(target) {
    this.pulseStart = performance.now()
    this.pulseTarget = target
  },

  // ── Animation loop ──────────────────────────────────────────────────────

  animate() {
    if (this.disposed) return
    requestAnimationFrame(() => this.animate())
    const now = performance.now()
    const dt = this.lastT ? (now - this.lastT) / 1000 : 0.016
    this.lastT = now

    if (this.orchestrator) {
      this.orchestrator.rotation.y += dt * 0.25
      this.orchestrator.rotation.x += dt * 0.08
      // Pulse on completion
      let scale = 1
      if (this.pulseStart) {
        const elapsed = (now - this.pulseStart) / 1000
        if (elapsed < 1.5) {
          scale = 1 + Math.sin(elapsed * Math.PI * 2) * 0.12 * Math.max(0, 1 - elapsed / 1.5)
        } else {
          this.pulseStart = null
        }
      }
      // Breathing scale tied to progress
      const breath = 1 + Math.sin(now / 700) * 0.02 * (this.agentState.progress || 0)
      this.orchestrator.scale.setScalar(scale * breath)
    }

    if (this.reactorInner && this.reactorTargetY !== undefined) {
      const cur = this.reactorInner.scale.y
      this.reactorInner.scale.y = cur + (this.reactorTargetY - cur) * Math.min(1, dt * 2.5)
    }

    // Animate satellites
    const stale = []
    for (const s of this.satellites) {
      const u = s.userData
      u.angle += u.angularVel * dt
      s.position.set(
        Math.cos(u.angle) * u.radius,
        Math.sin(u.angle * 0.7) * u.tiltY,
        Math.sin(u.angle) * u.radius,
      )
      const age = now - u.bornAt
      if (age > u.ttl) stale.push(s)
      else {
        const fade = age > u.ttl - 3000 ? Math.max(0, (u.ttl - age) / 3000) : 1
        s.material.opacity = fade
        s.material.transparent = fade < 1
      }
    }
    for (const s of stale) {
      this.satelliteRoot.remove(s)
      const idx = this.satellites.indexOf(s)
      if (idx >= 0) this.satellites.splice(idx, 1)
      s.material.dispose()
    }
    if (stale.length && this.satCountEl) this.satCountEl.textContent = String(this.satellites.length)

    this.controls && this.controls.update()
    this.updateBubblePosition()
    if (this.composer) this.composer.render()
    else this.renderer.render(this.scene, this.camera)
  },

  handleResize() {
    const w = this.el.clientWidth, h = this.el.clientHeight
    if (!w || !h) return
    this.camera.aspect = w / h
    this.camera.updateProjectionMatrix()
    this.renderer.setSize(w, h)
    this.composer && this.composer.setSize(w, h)
  },
}

export default MissionControl
