// 15-resource GCP infrastructure stack — the canonical Stage 1 P-16
// demo fixture. Mirrors the kind of small-but-real estate the Mapper
// needs to handle E2E.
//
// Networking (4): 1 VPC + 3 subnets
// Firewall  (3): web, db, ssh rules
// Compute   (3): 2 web instances + 1 app instance
// Storage   (5): assets, logs, backups, artifacts, uploads buckets

resource "google_compute_network" "main" {
  name                    = "main"
  auto_create_subnetworks = false
}

resource "google_compute_subnetwork" "public_a" {
  name          = "public-a"
  ip_cidr_range = "10.0.1.0/24"
  network       = google_compute_network.main.self_link
  region        = "us-central1"
}

resource "google_compute_subnetwork" "public_b" {
  name          = "public-b"
  ip_cidr_range = "10.0.2.0/24"
  network       = google_compute_network.main.self_link
  region        = "us-central1"
}

resource "google_compute_subnetwork" "private_a" {
  name          = "private-a"
  ip_cidr_range = "10.0.10.0/24"
  network       = google_compute_network.main.self_link
  region        = "us-central1"
}

resource "google_compute_firewall" "web" {
  name    = "web"
  network = google_compute_network.main.self_link
  allow {
    protocol = "tcp"
    ports    = ["80", "443"]
  }
}

resource "google_compute_firewall" "db" {
  name    = "db"
  network = google_compute_network.main.self_link
  allow {
    protocol = "tcp"
    ports    = ["5432"]
  }
}

resource "google_compute_firewall" "ssh" {
  name    = "ssh"
  network = google_compute_network.main.self_link
  allow {
    protocol = "tcp"
    ports    = ["22"]
  }
}

resource "google_compute_instance" "web1" {
  name         = "web1"
  machine_type = "e2-small"
  zone         = "us-central1-a"
  network_interface {
    subnetwork = google_compute_subnetwork.public_a.self_link
  }
  boot_disk {
    initialize_params {
      image = "debian-cloud/debian-12"
    }
  }
}

resource "google_compute_instance" "web2" {
  name         = "web2"
  machine_type = "e2-small"
  zone         = "us-central1-b"
  network_interface {
    subnetwork = google_compute_subnetwork.public_b.self_link
  }
  boot_disk {
    initialize_params {
      image = "debian-cloud/debian-12"
    }
  }
}

resource "google_compute_instance" "app1" {
  name         = "app1"
  machine_type = "e2-medium"
  zone         = "us-central1-a"
  network_interface {
    subnetwork = google_compute_subnetwork.private_a.self_link
  }
  boot_disk {
    initialize_params {
      image = "debian-cloud/debian-12"
    }
  }
}

resource "google_storage_bucket" "assets" {
  name     = "myorg-assets"
  location = "US"
}

resource "google_storage_bucket" "logs" {
  name     = "myorg-logs"
  location = "US"
}

resource "google_storage_bucket" "backups" {
  name     = "myorg-backups"
  location = "US"
}

resource "google_storage_bucket" "artifacts" {
  name     = "myorg-artifacts"
  location = "US"
}

resource "google_storage_bucket" "uploads" {
  name     = "myorg-uploads"
  location = "US"
}
