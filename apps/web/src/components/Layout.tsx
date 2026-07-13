import type { ReactNode } from "react";

type Props = {
  sidebar: ReactNode;
  children: ReactNode;
};

export default function Layout({ sidebar, children }: Props) {
  return (
    <main className="web-app-shell">
      {sidebar}
      {children}
    </main>
  );
}
