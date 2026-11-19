/** @type {import('tailwindcss').Config} */
module.exports = {
    darkMode: ['class'],
    content: [
    './src/pages/**/*.{js,ts,jsx,tsx,mdx}',
    './src/components/**/*.{js,ts,jsx,tsx,mdx}',
    './src/app/**/*.{js,ts,jsx,tsx,mdx}',
  ],
  theme: {
  	extend: {
  		fontFamily: {
  			sans: [
  				'-apple-system',
  				'BlinkMacSystemFont',
  				'"SF Pro Text"',
  				'Inter',
  				'"Segoe UI"',
  				'Roboto',
  				'"Helvetica Neue"',
  				'Arial',
  				'sans-serif'
  			],
  			mono: [
  				'ui-monospace',
  				'"SF Mono"',
  				'Menlo',
  				'Consolas',
  				'monospace'
  			]
  		},
  		colors: {
  			background: 'hsl(var(--background) / <alpha-value>)',
  			foreground: 'hsl(var(--foreground) / <alpha-value>)',
  			border: 'hsl(var(--border) / <alpha-value>)',
  			input: 'hsl(var(--input) / <alpha-value>)',
  			ring: 'hsl(var(--ring) / <alpha-value>)',
  			primary: {
  				DEFAULT: 'hsl(var(--primary) / <alpha-value>)',
  				foreground: 'hsl(var(--primary-foreground) / <alpha-value>)'
  			},
  			secondary: {
  				DEFAULT: 'hsl(var(--secondary) / <alpha-value>)',
  				foreground: 'hsl(var(--secondary-foreground) / <alpha-value>)'
  			},
  			tertiary: 'hsl(var(--muted-foreground) / <alpha-value>)',
  			sidebar: {
  				DEFAULT: 'hsl(var(--sidebar) / <alpha-value>)',
  				foreground: 'hsl(var(--sidebar-foreground) / <alpha-value>)',
  				accent: 'hsl(var(--sidebar-accent) / <alpha-value>)',
  				border: 'hsl(var(--sidebar-border) / <alpha-value>)'
  			},
  			brand: {
  				DEFAULT: 'hsl(var(--brand) / <alpha-value>)',
  				soft: 'hsl(var(--brand-soft) / <alpha-value>)'
  			},
  			recording: 'hsl(var(--recording) / <alpha-value>)',
  			success: {
  				DEFAULT: 'hsl(var(--success) / <alpha-value>)',
  				soft: 'hsl(var(--success-soft) / <alpha-value>)'
  			},
  			warning: {
  				DEFAULT: 'hsl(var(--warning) / <alpha-value>)',
  				soft: 'hsl(var(--warning-soft) / <alpha-value>)'
  			},
  			danger: {
  				DEFAULT: 'hsl(var(--danger) / <alpha-value>)',
  				soft: 'hsl(var(--danger-soft) / <alpha-value>)'
  			},
  			card: {
  				DEFAULT: 'hsl(var(--card) / <alpha-value>)',
  				foreground: 'hsl(var(--card-foreground) / <alpha-value>)'
  			},
  			popover: {
  				DEFAULT: 'hsl(var(--popover) / <alpha-value>)',
  				foreground: 'hsl(var(--popover-foreground) / <alpha-value>)'
  			},
  			muted: {
  				DEFAULT: 'hsl(var(--muted) / <alpha-value>)',
  				foreground: 'hsl(var(--muted-foreground) / <alpha-value>)'
  			},
  			accent: {
  				DEFAULT: 'hsl(var(--accent) / <alpha-value>)',
  				foreground: 'hsl(var(--accent-foreground) / <alpha-value>)'
  			},
  			destructive: {
  				DEFAULT: 'hsl(var(--destructive) / <alpha-value>)',
  				foreground: 'hsl(var(--destructive-foreground) / <alpha-value>)'
  			},
  			chart: {
  				'1': 'hsl(var(--chart-1) / <alpha-value>)',
  				'2': 'hsl(var(--chart-2) / <alpha-value>)',
  				'3': 'hsl(var(--chart-3) / <alpha-value>)',
  				'4': 'hsl(var(--chart-4) / <alpha-value>)',
  				'5': 'hsl(var(--chart-5) / <alpha-value>)'
  			}
  		},
  		fontSize: {
  			'2xs': ['0.6875rem', { lineHeight: '1rem' }],
  			reading: ['15px', { lineHeight: '1.6' }]
  		},
  		transitionDuration: {
  			DEFAULT: '140ms'
  		},
  		borderRadius: {
  			lg: 'var(--radius)',
  			md: 'calc(var(--radius) - 2px)',
  			sm: 'calc(var(--radius) - 4px)'
  		},
  		keyframes: {
  			'accordion-down': {
  				from: {
  					height: '0'
  				},
  				to: {
  					height: 'var(--radix-accordion-content-height)'
  				}
  			},
  			'accordion-up': {
  				from: {
  					height: 'var(--radix-accordion-content-height)'
  				},
  				to: {
  					height: '0'
  				}
  			}
  		},
  		animation: {
  			'accordion-down': 'accordion-down 0.2s ease-out',
  			'accordion-up': 'accordion-up 0.2s ease-out'
  		}
  	}
  },
  plugins: [
    require("tailwindcss-animate"),
    require("@tailwindcss/typography"),
    require("@tailwindcss/container-queries"),
  ],
}