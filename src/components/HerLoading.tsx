import { useEffect, useRef, useCallback } from "react";
import * as THREE from "three";

const LENGTH = 30;
const RADIUS = 5.6;
const PI2 = Math.PI * 2;
const CANVAS_SIZE = 500;

function buildCurve(): THREE.CatmullRomCurve3 {
  const points: THREE.Vector3[] = [];
  const segments = 400;
  for (let i = 0; i <= segments; i++) {
    const t = i / segments;
    let x = LENGTH * Math.sin(PI2 * t);
    let y = RADIUS * Math.cos(PI2 * 3 * t);
    let tt = (t % 0.25) / 0.25;
    tt = (t % 0.25) - (2 * (1 - tt) * tt * -0.0185 + tt * tt * 0.25);
    if (Math.floor(t / 0.25) === 0 || Math.floor(t / 0.25) === 2) {
      tt *= -1;
    }
    let z = RADIUS * Math.sin(PI2 * 2 * (t - tt));
    points.push(new THREE.Vector3(x, y, z));
  }
  return new THREE.CatmullRomCurve3(points, true);
}

interface HerLoadingProps {
  onEnter?: () => void;
  progress: number; // 0..1 — when it reaches 1 the enter animation plays
}

export function HerLoading({ onEnter, progress }: HerLoadingProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const sceneData = useRef<{
    camera: THREE.PerspectiveCamera;
    scene: THREE.Scene;
    renderer: THREE.WebGLRenderer;
    group: THREE.Group;
    mesh: THREE.Mesh<THREE.TubeGeometry, THREE.MeshBasicMaterial>;
    ring: THREE.Mesh<THREE.RingGeometry, THREE.MeshBasicMaterial>;
    ringcover: THREE.Mesh<THREE.PlaneGeometry, THREE.MeshBasicMaterial>;
  } | null>(null);
  const anim = useRef({
    step: 0,
    entered: false,
    finished: false,
  });
  const onEnterRef = useRef(onEnter);
  onEnterRef.current = onEnter;

  const initScene = useCallback(() => {
    const el = containerRef.current;
    if (!el) return;

    const camera = new THREE.PerspectiveCamera(65, 1, 1, 10000);
    camera.position.z = 150;

    const scene = new THREE.Scene();
    const group = new THREE.Group();
    scene.add(group);

    const curve = buildCurve();
    const tubeGeo = new THREE.TubeGeometry(curve, 200, 1.1, 2, true);
    const meshMat = new THREE.MeshBasicMaterial({
      color: 0xffffff,
      transparent: true,
      opacity: 1,
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

    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setPixelRatio(window.devicePixelRatio);
    renderer.setSize(CANVAS_SIZE, CANVAS_SIZE);
    renderer.setClearColor(0x000000, 0);

    el.appendChild(renderer.domElement);

    sceneData.current = {
      camera,
      scene,
      renderer,
      group,
      mesh,
      ring,
      ringcover,
    };
  }, []);

  useEffect(() => {
    initScene();
    return () => {
      const data = sceneData.current;
      if (data) {
        data.renderer.dispose();
        if (data.renderer.domElement.parentElement) {
          data.renderer.domElement.parentElement.removeChild(
            data.renderer.domElement,
          );
        }
        sceneData.current = null;
      }
    };
  }, [initScene]);

  useEffect(() => {
    let rafId: number;
    const a = anim.current;

    function easing(t: number, b: number, c: number, d: number): number {
      t /= d / 2;
      if (t < 1) return (c / 2) * t * t + b;
      t -= 2;
      return (c / 2) * (t * t * t + 2) + b;
    }

    function animate() {
      const d = sceneData.current;
      if (!d) {
        rafId = requestAnimationFrame(animate);
        return;
      }

      if (!a.finished) {
        a.step = Math.max(
          0,
          Math.min(240, a.entered ? a.step + 4 : a.step - 6),
        );

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
        } else {
          d.group.rotation.y = 0;
          d.group.position.z = 0;
          d.mesh.material.opacity = 1;
          d.ringcover.material.opacity = 0;
          d.ring.material.opacity = 0;
        }

        d.mesh.rotation.x += 0.035 + acceleration;

        if (a.entered && acceleration >= 1) {
          a.finished = true;
          rafId = requestAnimationFrame(animate);
          return;
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

  useEffect(() => {
    if (progress >= 1 && !anim.current.entered) {
      anim.current.entered = true;
    }
  }, [progress]);

  return (
    <div
      ref={containerRef}
      className="fixed inset-0 flex items-center justify-center bg-black z-50"
      style={{ overflow: "hidden" }}
    />
  );
}
