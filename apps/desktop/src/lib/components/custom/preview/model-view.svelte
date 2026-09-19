<script lang="ts">
  import * as THREE from "three";
  import { ColladaLoader } from "three/examples/jsm/loaders/ColladaLoader.js";
  import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
  import { OBJLoader } from "three/examples/jsm/loaders/OBJLoader.js";
  import { STLLoader } from "three/examples/jsm/loaders/STLLoader.js";
  import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
  import ArrowLeft from "@lucide/svelte/icons/arrow-left";
  import ArrowRight from "@lucide/svelte/icons/arrow-right";
  import ArrowUp from "@lucide/svelte/icons/arrow-up";
  import ArrowDown from "@lucide/svelte/icons/arrow-down";
  import Minus from "@lucide/svelte/icons/minus";
  import Plus from "@lucide/svelte/icons/plus";
  import RotateCcw from "@lucide/svelte/icons/rotate-ccw";

  type Props = { src: string; name: string };
  let { src, name }: Props = $props();

  let host = $state<HTMLElement | null>(null);
  let error = $state("");
  let loaded = $state(false);

  let camera: THREE.PerspectiveCamera | null = null;
  let controls: OrbitControls | null = null;
  let home = { position: new THREE.Vector3(), target: new THREE.Vector3() };

  function rotate(deltaTheta: number, deltaPhi: number) {
    if (!camera || !controls) return;
    const offset = camera.position.clone().sub(controls.target);
    const spherical = new THREE.Spherical().setFromVector3(offset);
    spherical.theta += deltaTheta;
    spherical.phi = Math.min(Math.PI - 0.05, Math.max(0.05, spherical.phi + deltaPhi));
    camera.position.copy(controls.target).add(offset.setFromSpherical(spherical));
    controls.update();
  }

  function pan(dx: number, dy: number) {
    if (!camera || !controls) return;
    const distance = camera.position.distanceTo(controls.target);
    const right = new THREE.Vector3().setFromMatrixColumn(camera.matrix, 0).multiplyScalar(dx * distance);
    const up = new THREE.Vector3().setFromMatrixColumn(camera.matrix, 1).multiplyScalar(dy * distance);
    controls.target.add(right).add(up);
    camera.position.add(right).add(up);
    controls.update();
  }

  function zoom(factor: number) {
    if (!camera || !controls) return;
    const offset = camera.position.clone().sub(controls.target);
    const distance = Math.max(offset.length() * factor, 1e-4);
    camera.position.copy(controls.target).add(offset.setLength(distance));
    controls.update();
  }

  function reset() {
    if (!camera || !controls) return;
    camera.position.copy(home.position);
    controls.target.copy(home.target);
    controls.update();
  }

  $effect(() => {
    const element = host;
    const url = src;
    if (!element || !url) return;

    let disposed = false;
    error = "";
    loaded = false;

    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0x1f1d1b);
    const viewCamera = new THREE.PerspectiveCamera(50, 1, 0.01, 100000);
    camera = viewCamera;
    const renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setPixelRatio(window.devicePixelRatio);
    element.appendChild(renderer.domElement);

    scene.add(new THREE.AmbientLight(0xffffff, 0.9));
    const key = new THREE.DirectionalLight(0xffffff, 1.6);
    key.position.set(4, 6, 5);
    scene.add(key);
    const fill = new THREE.DirectionalLight(0xffffff, 0.6);
    fill.position.set(-5, -2, -4);
    scene.add(fill);

    const grid = new THREE.GridHelper(10, 10, 0x555555, 0x333333);
    scene.add(grid);

    const orbit = new OrbitControls(viewCamera, renderer.domElement);
    controls = orbit;
    orbit.enableDamping = true;
    orbit.dampingFactor = 0.1;
    orbit.screenSpacePanning = true;
    orbit.rotateSpeed = 0.9;
    orbit.zoomSpeed = 0.9;
    orbit.touches = { ONE: THREE.TOUCH.ROTATE, TWO: THREE.TOUCH.DOLLY_PAN };

    function resize() {
      if (!element) return;
      const width = element.clientWidth || 1;
      const height = element.clientHeight || 1;
      renderer.setSize(width, height, false);
      viewCamera.aspect = width / height;
      viewCamera.updateProjectionMatrix();
    }
    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(element);

    function fit(object: THREE.Object3D) {
      const box = new THREE.Box3().setFromObject(object);
      const size = box.getSize(new THREE.Vector3());
      const center = box.getCenter(new THREE.Vector3());
      const max = Math.max(size.x, size.y, size.z) || 1;
      object.position.sub(center);
      viewCamera.position.set(max * 0.9, max * 0.75, max * 1.5);
      viewCamera.near = max / 100;
      viewCamera.far = max * 100;
      viewCamera.updateProjectionMatrix();
      orbit.target.set(0, 0, 0);
      orbit.update();
      grid.scale.setScalar(Math.max(1, max / 8));
      home = { position: viewCamera.position.clone(), target: orbit.target.clone() };
      loaded = true;
    }

    function done(object: THREE.Object3D) {
      if (disposed) return;
      scene.add(object);
      fit(object);
    }

    function fail(reason: unknown) {
      if (disposed) return;
      error = reason instanceof Error ? reason.message : String(reason);
    }

    const extension = name.split(".").pop()?.toLowerCase() ?? "";
    if (extension === "glb" || extension === "gltf") {
      new GLTFLoader().load(url, (gltf) => done(gltf.scene), undefined, fail);
    } else if (extension === "stl") {
      new STLLoader().load(
        url,
        (geometry) => {
          geometry.computeVertexNormals();
          done(
            new THREE.Mesh(
              geometry,
              new THREE.MeshStandardMaterial({ color: 0x9fb8e8, roughness: 0.7, metalness: 0.1 }),
            ),
          );
        },
        undefined,
        fail,
      );
    } else if (extension === "obj") {
      new OBJLoader().load(url, (object) => done(object), undefined, fail);
    } else if (extension === "dae") {
      new ColladaLoader().load(
        url,
        (collada) => {
          if (collada?.scene) done(collada.scene);
        },
        undefined,
        fail,
      );
    } else {
      fail(new Error("Unsupported 3D format"));
    }

    renderer.setAnimationLoop(() => {
      orbit.update();
      renderer.render(scene, viewCamera);
    });

    return () => {
      disposed = true;
      observer.disconnect();
      renderer.setAnimationLoop(null);
      orbit.dispose();
      renderer.dispose();
      camera = null;
      controls = null;
      if (renderer.domElement.parentNode === element) {
        element.removeChild(renderer.domElement);
      }
    };
  });

  const buttonClass =
    "grid size-8 place-items-center rounded-md border border-[#3a3734] bg-[#2a2825]/90 text-[#c0bbb5] backdrop-blur hover:bg-[#3b3836] hover:text-white disabled:opacity-40";
</script>

<div class="relative h-full min-h-0 bg-[#1f1d1b]" aria-label={`3D preview of ${name}`}>
  <div class="h-full w-full" bind:this={host}></div>
  {#if error}
    <div class="absolute inset-0 grid place-content-center justify-items-center px-4 text-center">
      <p class="text-[13px] text-[#9c9895]">{error}</p>
    </div>
  {:else if !loaded}
    <div class="pointer-events-none absolute inset-0 grid place-items-center">
      <span class="size-6 animate-spin rounded-full border-2 border-white/20 border-t-white/70" aria-label="Loading"></span>
    </div>
  {/if}

  {#if loaded && !error}
    <div class="absolute right-3 bottom-3 flex flex-col gap-2">
      <div class="grid grid-cols-3 grid-rows-3 gap-1" role="group" aria-label="Girar">
        <span></span>
        <button class={buttonClass} title="Girar arriba" aria-label="Girar arriba" onclick={() => rotate(0, -0.25)}>
          <ArrowUp class="size-4" />
        </button>
        <span></span>
        <button class={buttonClass} title="Girar izquierda" aria-label="Girar izquierda" onclick={() => rotate(-0.25, 0)}>
          <ArrowLeft class="size-4" />
        </button>
        <button class={buttonClass} title="Reiniciar vista" aria-label="Reiniciar vista" onclick={reset}>
          <RotateCcw class="size-4" />
        </button>
        <button class={buttonClass} title="Girar derecha" aria-label="Girar derecha" onclick={() => rotate(0.25, 0)}>
          <ArrowRight class="size-4" />
        </button>
        <span></span>
        <button class={buttonClass} title="Girar abajo" aria-label="Girar abajo" onclick={() => rotate(0, 0.25)}>
          <ArrowDown class="size-4" />
        </button>
        <span></span>
      </div>
      <div class="grid grid-cols-3 gap-1" role="group" aria-label="Desplazar">
        <button class={buttonClass} title="Mover izquierda" aria-label="Mover izquierda" onclick={() => pan(-0.1, 0)}>
          <ArrowLeft class="size-4" />
        </button>
        <button class={buttonClass} title="Mover arriba" aria-label="Mover arriba" onclick={() => pan(0, 0.1)}>
          <ArrowUp class="size-4" />
        </button>
        <button class={buttonClass} title="Mover derecha" aria-label="Mover derecha" onclick={() => pan(0.1, 0)}>
          <ArrowRight class="size-4" />
        </button>
      </div>
      <div class="grid grid-cols-2 gap-1" role="group" aria-label="Zoom">
        <button class={buttonClass} title="Acercar" aria-label="Acercar" onclick={() => zoom(0.8)}>
          <Plus class="size-4" />
        </button>
        <button class={buttonClass} title="Alejar" aria-label="Alejar" onclick={() => zoom(1.25)}>
          <Minus class="size-4" />
        </button>
      </div>
    </div>
  {/if}

  <p class="pointer-events-none absolute bottom-2 left-2 rounded bg-black/40 px-2 py-0.5 text-[11px] text-[#c0bbb5]">
    Arrastra para girar · botones abajo a la derecha
  </p>
</div>
