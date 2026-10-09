//! World parameters, saved in `world.json`. Every struct uses `serde(default)`
//! so older project files keep loading as parameters are added.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlanetParams {
    pub seed: u64,
    /// Icosphere subdivision level (6 = preview, 7 = fast, 8 = default, 9 = high detail).
    pub grid_level: u32,
    pub radius_km: f64,
    pub axial_tilt_deg: f64,
    pub day_length_h: f64,
    pub year_length_days: f64,
    pub eccentricity: f64,
    /// Solar longitude of perihelion, degrees (Earth ≈ 283°, i.e. early January).
    pub perihelion_deg: f64,
    pub solar_constant: f64,
    /// 1.0 = Earth. Each doubling adds about 30 W/m² of extra greenhouse forcing.
    pub greenhouse: f64,
    /// Used when the continent sketch is generated automatically.
    pub ocean_fraction: f64,
}

impl Default for PlanetParams {
    fn default() -> Self {
        PlanetParams {
            seed: 1,
            grid_level: 8,
            radius_km: 6371.0,
            axial_tilt_deg: 23.4,
            day_length_h: 24.0,
            year_length_days: 365.25,
            eccentricity: 0.0167,
            perihelion_deg: 283.0,
            solar_constant: 1361.0,
            greenhouse: 1.0,
            ocean_fraction: 0.70,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SketchParams {
    /// Amplitude of coastline noise added to soft sketch edges (0 = exact brush edges).
    pub coast_roughness: f64,
    /// Feature scale of the coastline noise, in km.
    pub coast_scale_km: f64,
    /// Number of continents when the sketch is generated automatically (0 = noise only).
    pub auto_continents: u32,
}

impl Default for SketchParams {
    fn default() -> Self {
        SketchParams { coast_roughness: 0.55, coast_scale_km: 900.0, auto_continents: 5 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlateParams {
    pub major_plates: u32,
    pub minor_plates: u32,
    /// Target land area per continental plate (million km²).
    pub continental_plate_area_mkm2: f64,
    /// Width of continental shelf/crust beyond the coastline (km).
    pub shelf_width_km: f64,
    /// Scales all plate speeds.
    pub speed_scale: f64,
    /// Irregularity of plate edges (0 = smooth Voronoi, 1 = very ragged).
    pub edge_roughness: f64,
}

impl Default for PlateParams {
    fn default() -> Self {
        PlateParams {
            major_plates: 12,
            minor_plates: 20,
            continental_plate_area_mkm2: 22.0,
            shelf_width_km: 120.0,
            speed_scale: 1.0,
            edge_roughness: 0.6,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TectonicParams {
    /// 0 = pure tectonic result, 1 = the sketched coastline always wins.
    pub sketch_fidelity: f64,
    pub mountain_scale: f64,
    pub detail_scale: f64,
    pub hotspots: u32,
    /// Strength of "mountains here" hint strokes (metres at full brush).
    pub hint_height_m: f64,
    pub max_influence_km: f64,
}

impl Default for TectonicParams {
    fn default() -> Self {
        TectonicParams {
            sketch_fidelity: 0.85,
            mountain_scale: 1.0,
            detail_scale: 1.0,
            hotspots: 8,
            hint_height_m: 3200.0,
            max_influence_km: 1600.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ClimateParams {
    /// Grid level the climate is solved on (capped at the world level).
    pub climate_level: u32,
    pub lapse_rate_c_per_km: f64,
    /// Global mean precipitation at Earth temperature (mm/year).
    pub global_precip_mm: f64,
    /// Strength of warm/cold coastal current offsets (°C at full effect).
    pub ocean_current_c: f64,
    /// How far maritime air reaches inland along the wind (km).
    pub maritime_reach_km: f64,
    /// Orographic rain strength.
    pub orographic: f64,
    pub moisture_passes: u32,
    /// Heat transport toward the poles (W/m²/K, North's D).
    pub heat_diffusion: f64,
    /// Strength of winds driven by thermal pressure systems (monsoons, continental highs).
    pub thermal_wind: f64,
    /// Fraction of upslope wind turned along the contours of steep terrain.
    pub mountain_blocking: f64,
}

impl Default for ClimateParams {
    fn default() -> Self {
        ClimateParams {
            climate_level: 7,
            lapse_rate_c_per_km: 6.5,
            global_precip_mm: 1000.0,
            ocean_current_c: 5.0,
            maritime_reach_km: 1400.0,
            orographic: 1.0,
            moisture_passes: 90,
            heat_diffusion: 0.68,
            thermal_wind: 1.0,
            mountain_blocking: 0.7,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HydroParams {
    /// Discharge above which a cell is a river (m³/s).
    pub river_threshold_m3s: f64,
    pub erosion_passes: u32,
    pub erosion_strength: f64,
    pub thermal_passes: u32,
    /// Talus slope for thermal erosion (metres of drop per km).
    pub talus_m_per_km: f64,
    /// Basins shallower than this after outlet incision hold no lake (m).
    pub lake_min_depth_m: f64,
    /// River width w = width_coeff · Q^width_exponent (m, with Q in m³/s).
    pub width_coeff: f64,
    pub width_exponent: f64,
    /// River systems shorter than this (source to mouth) are not drawn as rivers.
    pub min_river_length_km: f64,
    /// Share of a river's flow lost per 100 km across fully arid land.
    pub channel_loss: f64,
    /// How far outlet rivers cut into basin rims (m); shallower basins drain completely.
    pub breach_depth_m: f64,
    /// Open-water evaporation as a multiple of land potential evapotranspiration.
    pub open_water_evap: f64,
    /// Below-sea-level water bodies smaller than this (million km²) that are not the
    /// main ocean are inland basins whose level follows their water balance.
    pub inland_sea_max_mkm2: f64,
    /// Times the climate is re-solved with lakes as open water (0 = off).
    pub lake_climate_feedback: u32,
}

impl Default for HydroParams {
    fn default() -> Self {
        HydroParams {
            river_threshold_m3s: 300.0,
            erosion_passes: 4,
            erosion_strength: 1.0,
            thermal_passes: 3,
            talus_m_per_km: 45.0,
            lake_min_depth_m: 30.0,
            breach_depth_m: 150.0,
            width_coeff: 5.0,
            width_exponent: 0.5,
            channel_loss: 0.02,
            min_river_length_km: 120.0,
            open_water_evap: 1.25,
            inland_sea_max_mkm2: 1.0,
            lake_climate_feedback: 1,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BiomeParams {
    /// Coldest-month threshold between C and D climates (0 °C or -3 °C).
    pub cd_threshold_c: f64,
    pub mountain_elev_m: f64,
    pub hill_relief_m: f64,
    pub mountain_relief_m: f64,
    /// Annual precipitation above which temperate land is forest rather than plains.
    pub forest_precip_mm: f64,
}

impl Default for BiomeParams {
    fn default() -> Self {
        BiomeParams {
            cd_threshold_c: 0.0,
            mountain_elev_m: 1800.0,
            hill_relief_m: 300.0,
            mountain_relief_m: 900.0,
            forest_precip_mm: 650.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct HabitabilityParams {
    /// Distance over which a river still gives water and transport (km).
    pub river_reach_km: f64,
    /// Distance over which the coast still adds to habitability (km).
    pub coast_reach_km: f64,
    /// Barrier strength of ridge crests (cells higher than their surroundings).
    pub barrier_ridge: f64,
    /// Barrier strength of high ground in general.
    pub barrier_mountain: f64,
    /// Barrier strength of border rivers.
    pub barrier_river: f64,
    /// Barrier strength of deep desert away from rivers.
    pub barrier_desert: f64,
    /// Barrier strength of ice caps.
    pub barrier_ice: f64,
    /// Barrier strength of marsh (jungle counts for 60 %).
    pub barrier_marsh: f64,
    /// Rivers at least this wide (m) can act as borders.
    pub river_border_width_m: f64,
    /// Wide rivers through land at least this habitable divide it (Rhine).
    pub river_border_habitability: f64,
    /// Rivers through land drier than this (mm/yr, regional) hold their valley
    /// together instead (Nile).
    pub backbone_max_precip_mm: f64,
    /// Share of rain above 150 mm/yr that soaks in and becomes groundwater.
    pub groundwater_recharge: f64,
    /// Groundwater flows downhill underground; this far (km) it keeps 1/e of its water.
    pub groundwater_reach_km: f64,
    /// Spring flow (m³/s) that makes a full oasis.
    pub spring_flux_m3s: f64,
    /// Springs only matter in land drier than this (mm/yr).
    pub spring_max_precip_mm: f64,
    /// Habitability of a full oasis.
    pub spring_habitability: f64,
}

impl Default for HabitabilityParams {
    fn default() -> Self {
        HabitabilityParams {
            river_reach_km: 60.0,
            coast_reach_km: 120.0,
            barrier_ridge: 4.0,
            barrier_mountain: 2.5,
            barrier_river: 5.0,
            barrier_desert: 2.0,
            barrier_ice: 10.0,
            barrier_marsh: 1.5,
            river_border_width_m: 300.0,
            river_border_habitability: 0.3,
            backbone_max_precip_mm: 350.0,
            groundwater_recharge: 0.15,
            groundwater_reach_km: 400.0,
            spring_flux_m3s: 1.0,
            spring_max_precip_mm: 350.0,
            spring_habitability: 0.7,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct StateParams {
    /// Mean land area per state (km²).
    pub state_area_km2: f64,
    /// States smaller than this merge into the neighbour they share most border with.
    pub min_state_area_km2: f64,
    /// 0 = equal-sized states, 1 = much smaller states on fertile land.
    pub habitability_density: f64,
    /// Random head start of each seed, as a share of the state radius (size variety).
    pub size_variation: f64,
    /// Multiplies every barrier cost.
    pub barrier_weight: f64,
    /// Cost of crossing water per km, relative to open land.
    pub sea_crossing: f64,
    /// Islands at least this large (km²) become states of their own.
    pub island_state_km2: f64,
    pub states_per_region: f64,
    /// Landmasses at least this large (million km²) are continents.
    pub continent_min_mkm2: f64,
}

impl Default for StateParams {
    fn default() -> Self {
        StateParams {
            state_area_km2: 150_000.0,
            min_state_area_km2: 20_000.0,
            habitability_density: 0.8,
            size_variation: 0.35,
            barrier_weight: 1.0,
            sea_crossing: 8.0,
            island_state_km2: 40_000.0,
            states_per_region: 6.0,
            continent_min_mkm2: 2.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProvinceParams {
    /// Province area on fully habitable land (km²).
    pub province_area_km2: f64,
    /// Province area on barely habitable land (km²).
    pub sparse_province_area_km2: f64,
    /// Smaller provinces merge into a neighbour in the same state.
    pub min_province_area_km2: f64,
    /// Barrier weight inside states (lower than between states).
    pub barrier_weight: f64,
    pub lloyd_passes: u32,
    /// Land below this habitability (in large patches) becomes wasteland.
    pub wasteland_habitability: f64,
    /// Mountains above this elevation become wasteland.
    pub wasteland_elev_m: f64,
    pub wasteland_area_km2: f64,
    /// Sea within this distance of land is coastal sea.
    pub coastal_band_km: f64,
    /// Sea shallower than this (outside the coastal band) is shelf sea.
    pub shelf_depth_m: f64,
    pub coastal_sea_km2: f64,
    pub shelf_sea_km2: f64,
    pub open_sea_km2: f64,
    /// Lakes at least this large get their own province.
    pub lake_province_km2: f64,
    /// Longest water crossing written to adjacencies.csv (km).
    pub max_strait_km: f64,
}

impl Default for ProvinceParams {
    fn default() -> Self {
        ProvinceParams {
            province_area_km2: 12_000.0,
            sparse_province_area_km2: 150_000.0,
            min_province_area_km2: 2_500.0,
            barrier_weight: 0.4,
            lloyd_passes: 2,
            wasteland_habitability: 0.04,
            wasteland_elev_m: 3200.0,
            wasteland_area_km2: 120_000.0,
            coastal_band_km: 180.0,
            shelf_depth_m: 1500.0,
            coastal_sea_km2: 45_000.0,
            shelf_sea_km2: 160_000.0,
            open_sea_km2: 900_000.0,
            lake_province_km2: 6_000.0,
            max_strait_km: 160.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CultureParams {
    /// Bands placed at the start, weighted by carrying capacity.
    pub initial_bands: u32,
    /// Band count cap (never below the number of provinces that can hold people).
    pub max_bands: u32,
    /// Simulation length in ticks (one tick = one generation).
    pub ticks: u32,
    pub years_per_tick: f64,
    /// Cultural traits per band (Axelrod features) and values per trait.
    pub traits: u32,
    pub trait_values: u32,
    /// People per km² a fully habitable province supports.
    pub density_per_km2: f64,
    /// Logistic growth rate per tick.
    pub growth_rate: f64,
    /// Chance per trait that a band founded by fission starts with a changed value.
    pub fission_mutation: f64,
    /// Chance per trait and tick of a random change.
    pub drift: f64,
    /// Contacts each band tries per tick.
    pub contacts_per_tick: u32,
    /// A contact succeeds with probability similarity^homophily: higher values
    /// make unlike bands avoid each other, so cultures form sharper borders.
    pub homophily: f64,
    /// Chance per tick that a band takes, for one trait, the value most of its
    /// successful contacts share (conformist transmission).
    pub conformity: f64,
    /// Contact weight kept from one tick to the next.
    pub contact_memory: f64,
    /// Contact falls off as exp(−cost / scale): most contact is within about this distance.
    pub contact_scale_km: f64,
    /// Travel range (km of open land) in each of the four eras: the farthest a
    /// new band settles or a contact reaches.
    pub travel_range_km: [f64; 4],
    /// Start of eras 2–4 (straits, coastal sailing, open sea) as a fraction of the run.
    pub era_starts: [f64; 3],
    /// Cost multiplier for barriers along a border.
    pub barrier_weight: f64,
    /// Cost per km at sea (relative to open land) once sailing is possible.
    pub sea_cost: f64,
    /// Ticks between culture checks.
    pub check_every: u32,
    /// Checks in a row a split, merge or change of culture must hold.
    pub persistence: u32,
    /// Community detection resolution for cultures (higher = more, smaller cultures).
    pub culture_resolution: f64,
    /// Resolution for culture groups (lower = fewer, larger groups).
    pub group_resolution: f64,
    /// Smallest share of world population a new culture can start with.
    pub min_culture_share: f64,
    /// From the second era: people a watered stop on the busiest caravan route
    /// across dry land can hold (less on quieter routes).
    pub caravan_people: f64,
    /// From the third era: people each metal deposit draws to a mining town.
    pub mining_people: f64,
    /// From the fourth era: share of a dry province's land that irrigation
    /// from its river turns into farmland.
    pub irrigation_share: f64,
}

impl Default for CultureParams {
    fn default() -> Self {
        CultureParams {
            initial_bands: 20,
            max_bands: 5000,
            ticks: 400,
            years_per_tick: 25.0,
            traits: 24,
            trait_values: 4,
            density_per_km2: 2.0,
            growth_rate: 0.3,
            fission_mutation: 0.03,
            drift: 0.0005,
            contacts_per_tick: 4,
            homophily: 3.0,
            conformity: 1.0,
            contact_memory: 0.9,
            contact_scale_km: 200.0,
            travel_range_km: [500.0, 800.0, 1200.0, 1800.0],
            era_starts: [0.25, 0.5, 0.75],
            barrier_weight: 1.0,
            sea_cost: 0.7,
            check_every: 10,
            persistence: 3,
            culture_resolution: 4.0,
            group_resolution: 0.05,
            min_culture_share: 0.002,
            caravan_people: 25_000.0,
            mining_people: 4_000.0,
            irrigation_share: 0.3,
        }
    }
}

/// Stage 4: nations and history, from polity formation to the start date.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct NationParams {
    /// Year the first polities can form.
    pub start_year: i32,
    /// Year history stops: the map's start date (1910–1920 recommended).
    pub start_date: i32,
    /// Years per simulation step.
    pub years_per_step: u32,
    /// People a province needs before a polity can form there.
    pub found_population: f64,
    /// Chance per step that a province with that many people and no ruler
    /// founds a polity (more people, likelier).
    pub found_rate: f64,
    /// Expansion attempts per nation and step at aggression 1.
    pub expansion_rate: f64,
    /// Cost multiplier for barriers along a border (ridges, rivers, desert).
    pub barrier_weight: f64,
    /// Extra cost of taking land of another culture (half for the same group).
    pub culture_weight: f64,
    /// Distance from the capital (km) at which expansion costs twice as much.
    pub reach_km: f64,
    /// Chance per step of a breakup at instability 1 (mixed cultures, size, spread).
    pub collapse_rate: f64,
    /// Share of a province's other cultures that takes its ruler's culture per year.
    pub assimilation: f64,
    /// Population growth per year before and during the industrial era.
    pub growth: f64,
    pub industrial_growth: f64,
    /// Era start years: gunpowder states, ocean shipping (overseas colonies),
    /// industry (railways, mining).
    pub gunpowder_year: i32,
    pub shipping_year: i32,
    pub industrial_year: i32,
    /// Farthest a colony can lie from its nation's coast (km), with ocean shipping.
    pub overseas_km: f64,
    /// Largest cities each nation links by rail.
    pub railway_cities: u32,
    /// Years between a nation's railway projects.
    pub railway_every_years: u32,
    /// People a railway station draws (railway towns, also in the desert).
    pub station_people: f64,
    /// Attraction (0–1) a capital grows into: people move there and it holds more.
    pub capital_pull: f64,
    /// Years a new capital takes to grow into its full pull.
    pub capital_years: f64,
    /// Share of a conquered province's people lost in the sack (four times for a capital).
    pub war_sack: f64,
    /// Attraction lost by a conquered province (devastation: people leave).
    pub war_devastation: f64,
    /// Share of devastation that heals each year.
    pub recovery: f64,
    /// Share of a nation's people that move each year toward its attractive
    /// provinces (capitals, stations, city pins), more from devastated land.
    pub migration: f64,
}

impl Default for NationParams {
    fn default() -> Self {
        NationParams {
            start_year: 800,
            start_date: 1914,
            years_per_step: 2,
            found_population: 60_000.0,
            found_rate: 0.0015,
            expansion_rate: 0.8,
            barrier_weight: 1.0,
            culture_weight: 1.5,
            reach_km: 700.0,
            collapse_rate: 0.01,
            assimilation: 0.0015,
            growth: 0.0018,
            industrial_growth: 0.009,
            gunpowder_year: 1450,
            shipping_year: 1500,
            industrial_year: 1830,
            overseas_km: 9000.0,
            railway_cities: 8,
            railway_every_years: 8,
            station_people: 15_000.0,
            capital_pull: 0.5,
            capital_years: 60.0,
            war_sack: 0.03,
            war_devastation: 0.25,
            recovery: 0.03,
            migration: 0.002,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct WorldParams {
    pub planet: PlanetParams,
    pub sketch: SketchParams,
    pub plates: PlateParams,
    pub tectonics: TectonicParams,
    pub climate: ClimateParams,
    pub hydrology: HydroParams,
    pub biomes: BiomeParams,
    pub habitability: HabitabilityParams,
    pub states: StateParams,
    pub provinces: ProvinceParams,
    pub cultures: CultureParams,
    pub nations: NationParams,
}

impl WorldParams {
    pub fn climate_level(&self) -> u32 {
        self.climate.climate_level.min(self.planet.grid_level).max(4)
    }
}
