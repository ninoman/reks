/** Settings → Mixer: the EQ layout, the mixer view and the Simple view's filters. */
import type { MixerEq, MixerFilters, MixerView } from "../../lib/ipc";
import { useSettings } from "../../store/settings";
import { ChoiceTile, ChoiceTiles } from "../ui/ChoiceTiles";
import { Field } from "../ui/Field";
import { Switch } from "../ui/Switch";
import { EqPreview } from "./previews/EqPreview";
import { MixerViewPreview } from "./previews/MixerViewPreview";
import { SettingItem, settingItemClass } from "./SettingItem";

const EQS: { id: MixerEq; label: string; hint: string }[] = [
  { id: "fourBand", label: "4-band", hint: "HI, HM, LM, LO and filters X and Y." },
  {
    id: "threeBand",
    label: "3-band",
    hint: "HI, MID, LOW and an FX knob per channel: filter, echo or reverb.",
  },
];

const VIEWS: { id: MixerView; label: string; hint: string }[] = [
  { id: "simple", label: "Simple", hint: "Channel faders only: no crossfader or assigns." },
  { id: "advanced", label: "Advanced", hint: "Every control, the crossfader and assigns too." },
];

export function MixerSettings({
  eq,
  view,
  filters,
}: {
  eq: MixerEq;
  view: MixerView;
  filters: MixerFilters;
}) {
  const { update } = useSettings.getState();
  return (
    <>
      <SettingItem title="EQ" hint="Each EQ keeps its own knob positions.">
        <ChoiceTiles
          aria-label="EQ"
          value={eq}
          onValueChange={(v) => void update({ mixerEq: v as MixerEq })}
        >
          {EQS.map((e) => (
            <ChoiceTile
              key={e.id}
              value={e.id}
              label={e.label}
              hint={e.hint}
              isDefault={e.id === "fourBand"}
              preview={<EqPreview eq={e.id} />}
            />
          ))}
        </ChoiceTiles>
      </SettingItem>
      <SettingItem title="Mixer view" hint="For either EQ. The sound follows what you see.">
        <ChoiceTiles
          aria-label="Mixer view"
          value={view}
          onValueChange={(v) => void update({ mixerView: v as MixerView })}
        >
          {VIEWS.map((m) => (
            <ChoiceTile
              key={m.id}
              value={m.id}
              label={m.label}
              hint={m.hint}
              isDefault={m.id === "simple"}
              preview={<MixerViewPreview view={m.id} />}
            />
          ))}
        </ChoiceTiles>
      </SettingItem>
      <FiltersSwitch filters={filters} eq={eq} view={view} />
    </>
  );
}

/** Shows what is in effect: off in 3-band (no filters), on in Advanced (always shown). */
function FiltersSwitch({
  filters,
  eq,
  view,
}: {
  filters: MixerFilters;
  eq: MixerEq;
  view: MixerView;
}) {
  const threeBand = eq === "threeBand";
  const advanced = view === "advanced";
  return (
    <div className={settingItemClass}>
      <Field
        label="Show filters in the Simple view"
        orientation="horizontal"
        hint={
          threeBand
            ? "The 3-band EQ has no filters X and Y: each channel's FX knob is its filter."
            : advanced
              ? "The Advanced view always shows both filters."
              : "Hidden filters are off. On by default."
        }
      >
        <Switch
          size="md"
          disabled={threeBand || advanced}
          checked={threeBand ? false : advanced ? true : filters === "show"}
          onCheckedChange={(on) =>
            void useSettings.getState().update({ mixerFilters: on ? "show" : "hide" })
          }
        />
      </Field>
    </div>
  );
}
