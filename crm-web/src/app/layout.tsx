import type { Metadata } from "next";
import "@/styles/globals.css";

export const metadata: Metadata = {
  title: "Trabix — Conversaciones",
  description: "Consola de conversaciones del bot (cliente ⇄ bot ⇄ asesor) — Trabix Granizados",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="es">
      <body>{children}</body>
    </html>
  );
}
