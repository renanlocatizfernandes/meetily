"use client"

import * as React from "react"

import { cn } from "@/lib/utils"
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip"

export interface IconButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  /** Nome acessível e texto da dica. */
  label: string
  tooltipSide?: "top" | "right" | "bottom" | "left"
  active?: boolean
  size?: "sm" | "md"
}

/** Botão só com ícone: sempre com `aria-label`, dica e alvo de pelo menos 28 px. */
const IconButton = React.forwardRef<HTMLButtonElement, IconButtonProps>(
  ({ label, tooltipSide = "bottom", active = false, size = "md", className, children, type = "button", ...props }, ref) => (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          ref={ref}
          type={type}
          aria-label={label}
          className={cn(
            "inline-flex shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors duration-150",
            "hover:bg-accent hover:text-foreground disabled:pointer-events-none disabled:opacity-50",
            "[&_svg]:size-4 [&_svg]:shrink-0",
            size === "sm" ? "h-7 w-7" : "h-8 w-8",
            active && "bg-accent text-foreground",
            className
          )}
          {...props}
        >
          {children}
        </button>
      </TooltipTrigger>
      <TooltipContent side={tooltipSide}>{label}</TooltipContent>
    </Tooltip>
  )
)
IconButton.displayName = "IconButton"

export { IconButton }
