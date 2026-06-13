import React, { useEffect, useRef } from "react";
import * as THREE from "three";

const SIZE = 72; // logical px

interface Avatar3DProps {
  phase: "idle" | "listening" | "thinking" | "seeing" | "speaking" | "done" | "error" | "hidden";
  micLevel: number; // 0..1 from the mic-level event
}

export const Avatar3D: React.FC<Avatar3DProps> = React.memo(({ phase, micLevel }) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const stateRef = useRef({ phase, micLevel });
  const spinRef = useRef(0);

  // Always keep state ref fresh so the animation loop reads latest without re-creating
  stateRef.current = { phase, micLevel };

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const dpr = Math.min(window.devicePixelRatio ?? 1, 2);
    canvas.width = SIZE * dpr;
    canvas.height = SIZE * dpr;
    canvas.style.width = `${SIZE}px`;
    canvas.style.height = `${SIZE}px`;

    // ── Renderer ──────────────────────────────────────────────
    const renderer = new THREE.WebGLRenderer({
      canvas,
      alpha: true,
      antialias: true,
      powerPreference: "low-power",
    });
    renderer.setPixelRatio(dpr);
    renderer.setSize(SIZE * dpr, SIZE * dpr, false);

    // ── Scene + Camera ────────────────────────────────────────
    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(45, 1, 0.1, 200);
    camera.position.z = 4.8;

    // ── Lighting ──────────────────────────────────────────────
    const ambient = new THREE.AmbientLight("#a78bfa", 1.4);
    scene.add(ambient);
    const key = new THREE.DirectionalLight("#ffffff", 3.0);
    key.position.set(1, 1.5, 2);
    scene.add(key);
    const rim = new THREE.DirectionalLight("#7c3aed", 2.0);
    rim.position.set(-1.5, -0.5, -1);
    scene.add(rim);

    // ── Pyramid (low-poly 4-sided cone) ───────────────────────
    const geo = new THREE.ConeGeometry(0.7, 1.1, 4, 1);
    const mat = new THREE.MeshPhysicalMaterial({
      color: "#7c3aed",
      emissive: "#4c1d95",
      emissiveIntensity: 0.4,
      roughness: 0.15,
      metalness: 0.5,
      clearcoat: 0.1,
    });
    const pyramid = new THREE.Mesh(geo, mat);
    scene.add(pyramid);

    // ── Glow orb (additive-blended sphere behind the pyramid) ─
    const glowGeo = new THREE.SphereGeometry(1.0, 32, 32);
    const glowMat = new THREE.MeshBasicMaterial({
      color: "#7c3aed",
      transparent: true,
      opacity: 0.12,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    const glow = new THREE.Mesh(glowGeo, glowMat);
    scene.add(glow);

    // ── Outer glow ring (idle ring) ───────────────────────────
    const ringGeo = new THREE.TorusGeometry(0.9, 0.02, 16, 64);
    const ringMat = new THREE.MeshBasicMaterial({
      color: "#a78bfa",
      transparent: true,
      opacity: 0.3,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    const ring = new THREE.Mesh(ringGeo, ringMat);
    ring.rotation.x = Math.PI / 2;
    scene.add(ring);

    // ── Eye points (two small spheres) ────────────────────────
    const eyeGeo = new THREE.SphereGeometry(0.08, 8, 8);
    const eyeMat = new THREE.MeshBasicMaterial({ color: "#ffffff" });
    const eyeL = new THREE.Mesh(eyeGeo, eyeMat);
    eyeL.position.set(-0.2, 0.25, 0.62);
    const eyeR = new THREE.Mesh(eyeGeo, eyeMat);
    eyeR.position.set(0.2, 0.25, 0.62);
    const eyeGroup = new THREE.Group();
    eyeGroup.add(eyeL, eyeR);
    pyramid.add(eyeGroup);

    // ── Animation loop ────────────────────────────────────────
    let raf: number;
    const clock = new THREE.Clock();

    function animate() {
      raf = requestAnimationFrame(animate);
      const dt = Math.min(clock.getDelta(), 0.1);
      const { phase, micLevel } = stateRef.current;

      // Base rotation
      let spinSpeed = 0.4;
      let glowPulse = 0.0;
      let shake = 0.0;

      switch (phase) {
        case "idle":
          spinSpeed = 0.3;
          glowPulse = 0.0;
          break;
        case "listening":
          spinSpeed = 0.3 + micLevel * 0.8;
          glowPulse = micLevel * 0.4;
          break;
        case "thinking":
          spinSpeed = 3.0;
          glowPulse = 0.25;
          break;
        case "speaking":
          spinSpeed = 0.1 + micLevel * 1.5;
          glowPulse = micLevel * 0.6;
          break;
        case "done":
          spinSpeed = 1.5;
          glowPulse = 0.15;
          break;
        case "error":
          spinSpeed = 0.2;
          glowPulse = 0.0;
          shake = 1.0;
          mat.color.set("#f87171");
          mat.emissive.set("#7f1d1d");
          break;
        default:
          break;
      }

      // Reset error color when not in error
      if (phase !== "error") {
        mat.color.set("#7c3aed");
        mat.emissive.set("#4c1d95");
      }

      // Spin
      spinRef.current += spinSpeed * dt;
      pyramid.rotation.y = spinRef.current;
      pyramid.rotation.x = Math.sin(spinRef.current * 0.5) * 0.1;
      ring.rotation.z += spinSpeed * 0.7 * dt;

      // Scale pulse from mic level
      const pulseTarget = 1 + micLevel * (phase === "speaking" ? 0.2 : 0.08);
      pyramid.scale.lerp(
        new THREE.Vector3(pulseTarget, pulseTarget, pulseTarget),
        0.3,
      );

      // Glow opacity
      glowMat.opacity = 0.06 + glowPulse;
      glow.scale.setScalar(1 + glowPulse * 0.6);

      // Ring
      ring.visible = phase !== "hidden";
      ringMat.opacity = 0.15 + glowPulse;

      // Shake on error
      if (shake > 0) {
        pyramid.position.x = Math.sin(performance.now() * 0.05) * 0.1 * shake;
        pyramid.position.y = Math.cos(performance.now() * 0.07) * 0.08 * shake;
      } else {
        pyramid.position.x = 0;
        pyramid.position.y = 0;
      }

      // Eye brightness on seeing phase
      if (phase === "seeing") {
        eyeMat.color.set("#ffffff");
        eyeMat.color.multiplyScalar(2.0);
      } else if (phase === "thinking") {
        eyeMat.color.set("#7c3aed");
      } else {
        eyeMat.color.set("#e2e8f0");
      }

      renderer.render(scene, camera);
    }

    animate();

    return () => {
      cancelAnimationFrame(raf);
      renderer.dispose();
      geo.dispose();
      mat.dispose();
      glowGeo.dispose();
      glowMat.dispose();
      ringGeo.dispose();
      ringMat.dispose();
      eyeGeo.dispose();
      eyeMat.dispose();
    };
  }, []);

  return (
    <canvas
      ref={canvasRef}
      style={{
        display: "block",
        borderRadius: "50%",
        flexShrink: 0,
      }}
    />
  );
});

export default Avatar3D;
