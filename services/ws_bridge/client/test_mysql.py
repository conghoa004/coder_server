
import pymysql
import sys

MYSQL_HOST = "127.0.0.1"
MYSQL_PORT = 13306
MYSQL_USER = "admin"
MYSQL_PASSWORD = "4b9cbe986808b806b779fc9114d31742"
MYSQL_DATABASE = None


def test_mysql():
    connection = None

    try:
        print("[INFO] Connecting to MySQL...")
        print(f"[INFO] Host: {MYSQL_HOST}:{MYSQL_PORT}")

        connection = pymysql.connect(
            host=MYSQL_HOST,
            port=MYSQL_PORT,
            user=MYSQL_USER,
            password=MYSQL_PASSWORD,
            database=MYSQL_DATABASE,
            connect_timeout=15,
            read_timeout=30,
            write_timeout=30,
            charset="utf8mb4",
            autocommit=True
        )

        print("[SUCCESS] MySQL connection established!")

        with connection.cursor() as cursor:
            # Check MySQL version
            cursor.execute("SELECT VERSION()")
            version = cursor.fetchone()[0]
            print(f"\nMySQL Version: {version}")

            # Check current connection
            cursor.execute("SELECT USER(), CURRENT_USER(), DATABASE()")
            user, current_user, database = cursor.fetchone()

            print(f"User: {user}")
            print(f"Authenticated as: {current_user}")
            print(f"Database: {database}")

            # List databases
            cursor.execute("SHOW DATABASES")
            databases = cursor.fetchall()

            print("\nAvailable Databases:")
            for db in databases:
                print(f"  - {db[0]}")

            # Execute test query
            cursor.execute("SELECT 1 + 1 AS result")
            result = cursor.fetchone()[0]

            print(f"\nTest Query: SELECT 1 + 1 = {result}")

            if result == 2:
                print("[SUCCESS] All tests passed!")

    except pymysql.MySQLError as e:
        print(f"\n[MYSQL ERROR] {e}")

    except Exception as e:
        print(f"\n[ERROR] {e}")

    finally:
        if connection:
            connection.close()
            print("\n[INFO] Connection closed.")


if __name__ == "__main__":
    test_mysql()
