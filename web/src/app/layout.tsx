import type { Metadata } from 'next';
import './globals.css';

export const metadata: Metadata = {
  title: 'Rumiga',
  description: 'Amiga emulator control panel',
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className="dark">
      <body className="bg-zinc-900 text-zinc-100 font-sans antialiased min-h-screen">
        <nav className="border-b border-zinc-800 px-4 py-3">
          <div className="max-w-4xl mx-auto flex items-center gap-6">
            <a href="/" className="text-lg font-bold text-amber-400">
              Rumiga
            </a>
            <a href="/files/" className="hover:text-amber-300">
              Files
            </a>
            <a href="/wifi/" className="hover:text-amber-300">
              WiFi
            </a>
            <a href="/machine/" className="hover:text-amber-300">
              Machine
            </a>
          </div>
        </nav>
        <main className="max-w-4xl mx-auto px-4 py-6">{children}</main>
      </body>
    </html>
  );
}
