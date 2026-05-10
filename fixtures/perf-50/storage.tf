# 10 S3 buckets. 10 of 50 source resources.

resource "aws_s3_bucket" "uploads_a" {
  bucket = "perf-50-uploads-a"

  tags = {
    Purpose = "user-uploads"
  }
}

resource "aws_s3_bucket" "uploads_b" {
  bucket = "perf-50-uploads-b"

  tags = {
    Purpose = "user-uploads"
  }
}

resource "aws_s3_bucket" "uploads_c" {
  bucket = "perf-50-uploads-c"

  tags = {
    Purpose = "user-uploads"
  }
}

resource "aws_s3_bucket" "logs_a" {
  bucket = "perf-50-logs-a"

  tags = {
    Purpose = "access-logs"
  }
}

resource "aws_s3_bucket" "logs_b" {
  bucket = "perf-50-logs-b"

  tags = {
    Purpose = "access-logs"
  }
}

resource "aws_s3_bucket" "logs_c" {
  bucket = "perf-50-logs-c"

  tags = {
    Purpose = "access-logs"
  }
}

resource "aws_s3_bucket" "backups_a" {
  bucket = "perf-50-backups-a"

  tags = {
    Purpose = "snapshot-backups"
  }
}

resource "aws_s3_bucket" "backups_b" {
  bucket = "perf-50-backups-b"

  tags = {
    Purpose = "snapshot-backups"
  }
}

resource "aws_s3_bucket" "backups_c" {
  bucket = "perf-50-backups-c"

  tags = {
    Purpose = "snapshot-backups"
  }
}

resource "aws_s3_bucket" "backups_d" {
  bucket = "perf-50-backups-d"

  tags = {
    Purpose = "snapshot-backups"
  }
}
