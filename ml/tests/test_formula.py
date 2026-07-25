from fatigue_lite.features import engineer_features
from fatigue_lite.formula import empirical_fatigue_score


def row(**updates):
    value = {
        "heart_rate_bpm": 72,
        "respiration_rate_bpm": 16,
        "rms_radial_speed_mps": 0.08,
        "moving_point_fraction": 0.6,
        "short_term_energy_mps2": 0.005,
        "long_term_energy_mps2": 0.005,
        "baseline_heart_rate_bpm": 72,
        "baseline_respiration_rate_bpm": 16,
    }
    value.update(updates)
    return value


def test_more_concordant_slowdown_and_quietness_scores_higher():
    alert = empirical_fatigue_score(row())
    tired = empirical_fatigue_score(
        row(
            heart_rate_bpm=62,
            respiration_rate_bpm=12,
            rms_radial_speed_mps=0.01,
            moving_point_fraction=0.1,
            short_term_energy_mps2=0.001,
        )
    )
    assert 0 <= alert < tired <= 100


def test_features_are_bounded_for_extreme_but_plausible_inputs():
    values = engineer_features(
        row(heart_rate_bpm=30, respiration_rate_bpm=4, rms_radial_speed_mps=5)
    )
    assert values[:2] == [2.0, 2.0]
    assert values[2] == 0.0

