"use client"

import { Switch } from "./ui/switch"
import { FlaskConical, AlertCircle } from "lucide-react"
import { useConfig } from "@/contexts/ConfigContext"
import {
  BetaFeatureKey,
  BETA_FEATURE_NAMES,
  BETA_FEATURE_DESCRIPTIONS
} from "@/types/betaFeatures"

export function BetaSettings() {
  const { betaFeatures, toggleBetaFeature } = useConfig();

  // Define feature order for display (allows custom ordering)
  const featureOrder: BetaFeatureKey[] = ['importAndRetranscribe'];

  return (
    <div className="mt-4 space-y-4 first:mt-0">
      {/* Yellow Warning Banner */}
      <div className="flex items-start gap-3 p-4 bg-warning-soft border border-warning/30 rounded-lg">
        <AlertCircle className="h-5 w-5 text-warning flex-shrink-0 mt-0.5" />
        <div className="text-sm text-warning">
          <p className="font-medium">Recursos beta</p>
          <p className="mt-1">
            Estes recursos ainda estão em teste. Podem ocorrer problemas, e seu feedback ajuda.
          </p>
        </div>
      </div>

      {/* Dynamic Feature Toggles - Automatically renders all features */}
      {featureOrder.map((featureKey) => (
        <div
          key={featureKey}
          className="bg-card rounded-xl border border-border p-5"
        >
          <div className="flex items-center justify-between">
            <div className="flex-1">
              <div className="flex items-center gap-2">
                <FlaskConical className="h-4 w-4 text-muted-foreground" />
                <h2 className="text-sm font-medium">
                  {BETA_FEATURE_NAMES[featureKey]}
                </h2>
                <span className="px-2 py-0.5 text-xs font-medium bg-warning-soft text-warning rounded-full">
                  Beta
                </span>
              </div>
              <p className="mt-0.5 text-[13px] text-muted-foreground">
                {BETA_FEATURE_DESCRIPTIONS[featureKey]}
              </p>
            </div>

            <div className="ml-6">
              <Switch
                checked={betaFeatures[featureKey]}
                onCheckedChange={(checked) => toggleBetaFeature(featureKey, checked)}
              />
            </div>
          </div>
        </div>
      ))}

      {/* Info Box */}
      <div className="p-4 bg-brand-soft border border-primary/30 rounded-lg">
        <p className="text-sm text-brand">
          <strong>Observação:</strong> recursos beta desativados ficam ocultos. Suas reuniões continuam como estão.
        </p>
      </div>
    </div>
  );
}
