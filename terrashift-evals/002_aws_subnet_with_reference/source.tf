resource "google_compute_network" "main" {
  name                    = "main"
  auto_create_subnetworks = false
}

resource "google_compute_subnetwork" "a" {
  name          = "a"
  ip_cidr_range = "10.0.1.0/24"
  network       = google_compute_network.main.self_link
}
