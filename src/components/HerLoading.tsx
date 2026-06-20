import { useEffect, useRef } from "react";
import * as THREE from "three";

const LENGTH = 30;
const RADIUS = 5.6;
const PI2 = Math.PI * 2;
const CANVAS_SIZE = 500;

class HerCurve extends THREE.Curve<THREE.Vector3> {
  constructor() {
    super();
  }
  getPoint(
    percent: number,
    optionalTarget = new THREE.Vector3(),
  ): THREE.Vector3 {
    const x = LENGTH * Math.sin(PI2 * percent);
    const y = RADIUS * Math.cos(PI2 * 3 * percent);
    let t = (percent % 0.25) / 0.25;
    t = (percent % 0.25) - (2 * (1 - t) * t * -0.0185 + t * t * 0.25);
    if (Math.floor(percent / 0.25) === 0 || Math.floor(percent / 0.25) === 2) {
      t *= -1;
    }
    const z = RADIUS * Math.sin(PI2 * 2 * (percent - t));
    return optionalTarget.set(x, y, z);
  }
}

interface HerLoadingProps {
  onEnter?: () => void;
  progress: number;
}

export function HerLoading({ onEnter, progress }: HerLoadingProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const infoRef = useRef<HTMLParagraphElement>(null);
  const sceneRef = useRef<{
    camera: THREE.PerspectiveCamera;
    scene: THREE.Scene;
    renderer: THREE.WebGLRenderer;
    group: THREE.Group;
    mesh: THREE.Mesh<THREE.TubeGeometry, THREE.MeshBasicMaterial>;
    ring: THREE.Mesh<THREE.RingGeometry, THREE.MeshBasicMaterial>;
    ringcover: THREE.Mesh<THREE.PlaneGeometry, THREE.MeshBasicMaterial>;
  } | null>(null);
  const animRef = useRef({ step: 0, toend: false, finished: false });
  const onEnterRef = useRef(onEnter);
  onEnterRef.current = onEnter;

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    const camera = new THREE.PerspectiveCamera(65, 1, 1, 10000);
    camera.position.z = 150;

    const scene = new THREE.Scene();
    const group = new THREE.Group();
    scene.add(group);

    const curve = new HerCurve();
    const tubeGeo = new THREE.TubeGeometry(curve, 200, 1.1, 2, true);
    const meshMat = new THREE.MeshBasicMaterial({
      color: 0xffffff,
    });
    const mesh = new THREE.Mesh(tubeGeo, meshMat);
    group.add(mesh);

    const ringcover = new THREE.Mesh(
      new THREE.PlaneGeometry(50, 15),
      new THREE.MeshBasicMaterial({
        color: 0x000000,
        opacity: 0,
        transparent: true,
      }),
    );
    ringcover.position.x = LENGTH + 1;
    ringcover.rotation.y = Math.PI / 2;
    group.add(ringcover);

    const ring = new THREE.Mesh(
      new THREE.RingGeometry(4.3, 5.55, 32),
      new THREE.MeshBasicMaterial({
        color: 0xffffff,
        opacity: 0,
        transparent: true,
      }),
    );
    ring.position.x = LENGTH + 1.1;
    ring.rotation.y = Math.PI / 2;
    group.add(ring);

    for (let i = 0; i < 10; i++) {
      const plain = new THREE.Mesh(
        new THREE.PlaneGeometry(LENGTH * 2 + 1, RADIUS * 3),
        new THREE.MeshBasicMaterial({
          color: 0x000000,
          transparent: true,
          opacity: 0.13,
        }),
      );
      plain.position.z = -2.5 + i * 0.5;
      group.add(plain);
    }

    const renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setPixelRatio(window.devicePixelRatio);
    renderer.setSize(CANVAS_SIZE, CANVAS_SIZE);
    renderer.setClearColor(0x000000);

    el.appendChild(renderer.domElement);

    sceneRef.current = {
      camera,
      scene,
      renderer,
      group,
      mesh,
      ring,
      ringcover,
    };

    return () => {
      renderer.dispose();
      if (renderer.domElement.parentElement) {
        renderer.domElement.parentElement.removeChild(renderer.domElement);
      }
      sceneRef.current = null;
    };
  }, []);

  useEffect(() => {
    const a = animRef.current;

    function trigger() {
      a.toend = true;
    }

    document.addEventListener("click", trigger);
    document.addEventListener("touchstart", trigger);
    document.addEventListener("keydown", trigger);

    return () => {
      document.removeEventListener("click", trigger);
      document.removeEventListener("touchstart", trigger);
      document.removeEventListener("keydown", trigger);
    };
  }, []);

  useEffect(() => {
    let rafId: number;
    const a = animRef.current;

    function easing(t: number, b: number, c: number, d: number): number {
      t /= d / 2;
      if (t < 1) return (c / 2) * t * t + b;
      t -= 2;
      return (c / 2) * (t * t * t + 2) + b;
    }

    function animate() {
      const d = sceneRef.current;
      if (!d) {
        rafId = requestAnimationFrame(animate);
        return;
      }

      if (!a.finished) {
        a.step = Math.max(0, Math.min(240, a.toend ? a.step + 1 : a.step - 4));
        const acceleration = easing(a.step, 0, 1, 240);

        if (acceleration > 0.35) {
          const p = (acceleration - 0.35) / 0.65;
          d.group.rotation.y = (-Math.PI / 2) * p;
          d.group.position.z = 50 * p;
          const fadeP = Math.max(0, (acceleration - 0.97) / 0.03);
          d.mesh.material.opacity = 1 - fadeP;
          d.ringcover.material.opacity = fadeP;
          d.ring.material.opacity = fadeP;
          d.ring.scale.x = d.ring.scale.y = 0.9 + 0.1 * fadeP;
        }

        d.mesh.rotation.x += 0.035 + acceleration;

        if (acceleration >= 1) {
          a.finished = true;
        }
      }

      d.renderer.render(d.scene, d.camera);

      if (a.finished) {
        onEnterRef.current?.();
        return;
      }

      rafId = requestAnimationFrame(animate);
    }

    rafId = requestAnimationFrame(animate);
    return () => cancelAnimationFrame(rafId);
  }, []);

  return (
    <>
      <div
        ref={containerRef}
        className="fixed inset-0 z-50 flex items-center justify-center"
        style={{
          background: "#000",
          overflow: "hidden",
          userSelect: "none",
          WebkitUserSelect: "none",
        }}
      />
      <p
        ref={infoRef}
        className="fixed bottom-0 left-0 right-0 z-50 text-center text-xs leading-8"
        style={{ color: "#ccc" }}
      >
        Click or press any key to enter
      </p>
    </>
  );
}
