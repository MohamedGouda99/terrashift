# 3 S3 buckets. 3 of 15 source resources.

resource "aws_s3_bucket" "uploads" {
  bucket = "mvp-15-uploads"

  tags = {
    Purpose = "user-uploads"
  }
}

resource "aws_s3_bucket" "logs" {
  bucket = "mvp-15-logs"

  tags = {
    Purpose = "access-logs"
  }
}

resource "aws_s3_bucket" "backups" {
  bucket = "mvp-15-backups"

  tags = {
    Purpose = "snapshot-backups"
  }
}
