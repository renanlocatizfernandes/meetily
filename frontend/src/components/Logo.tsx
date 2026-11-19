import React from "react";
import Image from "next/image";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Dialog, DialogContent, DialogTitle, DialogTrigger } from "./ui/dialog";
import { VisuallyHidden } from "./ui/visually-hidden";
import { About } from "./About";

interface LogoProps {
  isCollapsed: boolean;
  doodlePath?: string | null;
  doodleDate?: string | null;
}

const Logo = React.forwardRef<HTMLButtonElement, LogoProps>(
  ({ isCollapsed, doodlePath, doodleDate }, ref) => {
    const [failedSrc, setFailedSrc] = React.useState<string | null>(null);
    const doodleSrc = doodlePath && doodleDate
      ? `${convertFileSrc(doodlePath)}?d=${doodleDate}`
      : null;
    return (
      <Dialog aria-describedby={undefined}>
        {isCollapsed ? (
          <DialogTrigger asChild>
            <button
              ref={ref}
              type="button"
              className="flex items-center justify-center mb-2 cursor-pointer bg-transparent border-none p-0 hover:opacity-80 transition-opacity"
              aria-label="About Meetily"
            >
              {doodleSrc && doodleSrc !== failedSrc ? (
                // eslint-disable-next-line @next/next/no-img-element
                <img
                  src={doodleSrc}
                  alt="Meetily"
                  width={40}
                  height={40}
                  className="w-10 h-10 object-contain"
                  onError={() => setFailedSrc(doodleSrc)}
                />
              ) : (
                <Image
                  src="/logo-collapsed.png"
                  alt="Meetily"
                  width={40}
                  height={40}
                  className="object-contain"
                  priority
                />
              )}
            </button>
          </DialogTrigger>
        ) : (
          <DialogTrigger asChild>
            <button
              ref={ref}
              type="button"
              className="inline-flex h-8 items-center gap-1.5 rounded-md px-1.5 text-sm font-semibold tracking-tight text-foreground transition-colors duration-150 hover:bg-sidebar-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              aria-label="About Meetily"
            >
              <span>Meetily</span>
              <i className="font-normal not-italic text-muted-foreground">Live</i>
            </button>
          </DialogTrigger>
        )}
        <DialogContent>
          <VisuallyHidden>
            <DialogTitle>Sobre o Meetily Live Plus</DialogTitle>
          </VisuallyHidden>
          <About />
        </DialogContent>
      </Dialog>
    );
  },
);

Logo.displayName = "Logo";

export default Logo;
